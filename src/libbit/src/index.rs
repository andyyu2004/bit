mod index_entry;

use flate2::Decompress;
pub use index_entry::*;

use crate::error::BitResult;
use crate::hash::{BitHash, BIT_HASH_SIZE};
use crate::io_ext::{HashWriter, ReadExt, WriteExt};
use crate::obj::{FileMode, Tree, TreeEntry};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::util;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::collections::btree_map::Values;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
use std::iter::{Copied, Peekable};
use std::ops::{Deref, DerefMut};
use std::path::Path;

// refer to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
#[derive(Debug, PartialEq, Clone, Default)]
pub struct BitIndex {
    pub header: BitIndexHeader,
    /// sorted by ascending by filepath (interpreted as unsigned bytes)
    /// ties broken by stage (a part of flags)
    // the link says `name` which usually refers to the hash
    // but it is sorted by filepath
    pub entries: BitIndexEntries,
    pub extensions: Vec<BitIndexExtension>,
}

impl<'a> IntoIterator for &'a BitIndex {
    type IntoIter = Copied<Values<'a, (BitPath, MergeStage), BitIndexEntry>>;
    type Item = BitIndexEntry;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.values().copied()
    }
}

impl BitIndex {
    fn create_tree(&self, _repo: &BitRepo) -> Tree {
        let entries = BTreeSet::new();
        Tree { entries }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = BitIndexEntry> + 'a {
        self.into_iter()
    }
}

impl BitIndex {
    /// find entry by path
    pub fn find_entry(&self, path: BitPath, stage: MergeStage) -> Option<&BitIndexEntry> {
        self.entries.get(&(path, stage))
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, entry: BitIndexEntry) {
        self.entries.insert((entry.filepath, entry.flags.stage()), entry);
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.keys().any(|(_, stage)| stage.is_merging())
    }

    /// builds a tree object from the current index entries
    pub fn build_tree(&self, repo: &BitRepo) -> BitResult<Tree> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }
        TreeBuilder::new(self, repo, self.entries.values()).build()
    }
}

struct TreeBuilder<'a, I: Iterator<Item = &'a BitIndexEntry>> {
    index: &'a BitIndex,
    repo: &'a BitRepo,
    index_entries: Peekable<I>,
    // count the number of blobs created (not subtrees)
    // should match the number of index entries
}

impl<'a, I: Iterator<Item = &'a BitIndexEntry>> TreeBuilder<'a, I> {
    pub fn new(index: &'a BitIndex, repo: &'a BitRepo, entries: I) -> Self {
        Self { index, repo, index_entries: entries.peekable() }
    }

    fn build_tree(&mut self, current_index_dir: impl AsRef<Path>, depth: usize) -> BitResult<Tree> {
        let mut entries = BTreeSet::new();
        let current_index_dir = current_index_dir.as_ref();
        while let Some(next_entry) = self.index_entries.peek() {
            let &&BitIndexEntry { mode, filepath, hash, .. } = next_entry;
            // if the depth is greater than the number of components in the filepath
            // then we need to `break` and go out one level
            let (curr_dir, segment) = match filepath.try_split_path_at(depth) {
                Some(x) => x,
                None => break,
            };

            if curr_dir.as_path() != current_index_dir {
                break;
            }

            let nxt_path = curr_dir.as_path().join(segment);
            if nxt_path == filepath.as_path() {
                // only keep the final segment of the path inside the tree entry
                assert!(entries.insert(TreeEntry { mode, path: segment, hash }));
                self.index_entries.next();
            } else {
                let subtree = self.build_tree(&nxt_path, 1 + depth)?;
                let hash = self.repo.write_obj(&subtree)?;

                assert!(entries.insert(TreeEntry { path: segment, mode: FileMode::DIR, hash }));
            }
        }
        Ok(Tree { entries })
    }

    pub fn build(mut self) -> BitResult<Tree> {
        self.build_tree("", 0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BitIndexHeader {
    signature: [u8; 4],
    version: u32,
    entryc: u32,
}

impl Default for BitIndexHeader {
    fn default() -> Self {
        Self { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 0 }
    }
}

// this should be an enum of the concrete extensions
// but I don't really care about the extensions currently
// and they are optional anyway
#[derive(Debug, PartialEq, Clone)]
pub struct BitIndexExtension {
    pub signature: [u8; 4],
    pub size: u32,
    pub data: Vec<u8>,
}

// could probably do with better variant names once I know whats going on
#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, TryFromPrimitive, Copy, Clone)]
#[repr(u8)]
pub enum MergeStage {
    /// not merging
    None   = 0,
    Stage1 = 1,
    Stage2 = 2,
    Stage3 = 3,
}

impl MergeStage {
    pub fn is_merging(self) -> bool {
        self as u8 > 0
    }
}

impl Display for MergeStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl PartialOrd for BitIndexEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitIndexEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.filepath.cmp(&other.filepath).then_with(|| self.stage().cmp(&other.stage()))
    }
}

impl BitIndex {
    fn parse_header(r: &mut dyn BufRead) -> BitResult<BitIndexHeader> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        assert_eq!(&signature, b"DIRC");
        let version = r.read_u32()?;
        assert_eq!(version, 2, "Only index format v2 is supported");
        let entryc = r.read_u32()?;

        Ok(BitIndexHeader { signature, version, entryc })
    }

    fn parse_extensions(mut buf: &[u8]) -> BitResult<Vec<BitIndexExtension>> {
        let mut extensions = vec![];
        while buf.len() > BIT_HASH_SIZE {
            let signature: [u8; 4] = buf[0..4].try_into().unwrap();
            let size = u32::from_be_bytes(buf[4..8].try_into().unwrap());
            let data = buf[8..8 + size as usize].to_vec();
            extensions.push(BitIndexExtension { signature, size, data });
            buf = &buf[8 + size as usize..];
        }
        Ok(extensions)
    }

    // this impl currently is not ideal as it basically has to read it twice
    // although the second time is reading from memory so maybe its not that bad?
    pub fn deserialize<R: Read>(mut r: R) -> BitResult<BitIndex> {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut r = BufReader::new(&buf[..]);
        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| BitIndexEntry::deserialize(&mut r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?
            .into();

        let mut remainder = vec![];
        assert!(r.read_to_end(&mut remainder)? >= BIT_HASH_SIZE);
        let extensions = Self::parse_extensions(&remainder)?;

        let bit_index = Self { header, entries, extensions };

        let (bytes, hash) = buf.split_at(buf.len() - BIT_HASH_SIZE);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = BitHash::from(hasher.finalize());
        let expected_hash = BitHash::new(hash.try_into().unwrap());
        assert_eq!(actual_hash, expected_hash);

        Ok(bit_index)
    }
}

impl Serialize for BitIndexHeader {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let Self { signature, version, entryc } = self;
        writer.write(signature)?;
        writer.write(&version.to_be_bytes())?;
        writer.write(&entryc.to_be_bytes())?;
        Ok(())
    }
}

impl Serialize for BitIndexExtension {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        writer.write_all(&self.signature)?;
        writer.write_u32(self.size)?;
        writer.write_all(&self.data)?;
        Ok(())
    }
}

impl Serialize for BitIndex {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let mut hash_writer = HashWriter::new_sha1(writer);
        self.header.serialize(&mut hash_writer)?;

        for entry in self.entries.values() {
            entry.serialize(&mut hash_writer)?;
        }

        for extension in &self.extensions {
            extension.serialize(&mut hash_writer)?;
        }

        let hash = BitHash::from(hash_writer.finalize_hash());
        hash_writer.write_bit_hash(&hash)?;
        Ok(())
    }
}

impl Deserialize for BitIndex {
    fn deserialize(r: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut r = BufReader::new(&buf[..]);
        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| BitIndexEntry::deserialize(&mut r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?
            .into();

        let mut remainder = vec![];
        assert!(r.read_to_end(&mut remainder)? >= BIT_HASH_SIZE);
        let extensions = Self::parse_extensions(&remainder)?;

        let bit_index = Self { header, entries, extensions };

        let (bytes, hash) = buf.split_at(buf.len() - BIT_HASH_SIZE);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = BitHash::from(hasher.finalize());
        let expected_hash = BitHash::new(hash.try_into().unwrap());
        assert_eq!(actual_hash, expected_hash);

        Ok(bit_index)
    }
}

#[cfg(test)]
mod tests;
