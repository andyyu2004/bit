mod index_entry;

use crate::error::BitResult;
use crate::hash::{BitHash, BIT_HASH_SIZE};
use crate::io_ext::{HashWriter, ReadExt, WriteExt};
use crate::iter::BitIterator;
use crate::lockfile::Lockfile;
use crate::obj::{FileMode, Tree, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::time::Timespec;
use crate::tls;
use crate::util;
pub use index_entry::*;
use itertools::Itertools;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
use std::iter::Peekable;
use std::ops::{Deref, DerefMut};
use std::path::Path;

const BIT_INDEX_HEADER_SIG: &[u8; 4] = b"DIRC";
const BIT_INDEX_VERSION: u32 = 2;

#[derive(Debug)]
pub struct BitIndex<'r> {
    pub repo: &'r BitRepo,
    // index file may not yet exist
    mtime: Option<Timespec>,
    inner: BitIndexInner,
}

impl<'r> Deref for BitIndex<'r> {
    type Target = BitIndexInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// refer to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
#[derive(Debug, PartialEq, Clone, Default)]
pub struct BitIndexInner {
    /// sorted by ascending by filepath (interpreted as unsigned bytes)
    /// ties broken by stage (a part of flags)
    // the link says `name` which usually refers to the hash
    // but it is sorted by filepath
    entries: RefCell<BitIndexEntries>,
    pub extensions: Vec<BitIndexExtension>,
}

impl BitIndexInner {
    pub fn new(entries: BitIndexEntries, extensions: Vec<BitIndexExtension>) -> Self {
        Self { entries: RefCell::new(entries), extensions }
    }
}

// impl<'a> IntoIterator for &'a BitIndexInner {
//     type IntoIter = Copied<Values<'a, (BitPath, MergeStage), BitIndexEntry>>;
//     type Item = BitIndexEntry;

//     fn into_iter(self) -> Self::IntoIter {
//         self.entries.borrow().values().copied()
//     }
// }

impl<'r> BitIndex<'r> {
    pub fn from_lockfile(repo: &'r BitRepo, lockfile: &Lockfile) -> BitResult<Self> {
        // not actually writing anything here, so we rollback
        // the lockfile is just to check that another process
        // is not currently writing to the index
        let inner = lockfile
            .file()
            .map(BitIndexInner::deserialize_unbuffered)
            .transpose()?
            .unwrap_or_default();
        let mtime = std::fs::metadata(repo.index_path()).as_ref().map(Timespec::mtime).ok();
        Ok(Self { repo, inner, mtime })
    }

    /// builds a tree object from the current index entries
    pub fn build_tree(&self) -> BitResult<Tree> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }
        TreeBuilder::new(self).build()
    }

    pub fn is_racy_entry(&self, entry: &BitIndexEntry) -> bool {
        self.mtime.map(|mtime| mtime <= entry.mtime).unwrap_or(false)
    }
}

type IndexIterator = impl Iterator<Item = BitIndexEntry>;

impl BitIndexInner {
    pub fn std_iter(&self) -> IndexIterator {
        self.entries.borrow().values().cloned().collect_vec().into_iter()
    }

    pub fn iter(&self) -> impl BitIterator {
        // this is pretty nasty, but I'm uncertain of a better way to dissociate the lifetime of
        // `self` from the returned iterator
        fallible_iterator::convert(self.std_iter().map(Ok))
    }

    /// find entry by path
    pub fn find_entry(&self, path: BitPath, stage: MergeStage) -> Option<BitIndexEntry> {
        self.entries.borrow().get(&(path, stage)).copied()
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&self, mut entry: BitIndexEntry) -> BitResult<()> {
        self.remove_collisions(&entry)?;
        if entry.hash.is_zero() {
            entry.hash = tls::REPO.with(|repo| repo.hash_blob(entry.filepath))?;
        }
        self.entries.borrow_mut().insert(entry.as_key(), entry);
        Ok(())
    }

    /// removes collisions where there was originally a file but was replaced by a directory
    fn remove_file_dir_collisions(&self, entry: &BitIndexEntry) -> BitResult<()> {
        //? only removing entries with no merge stage (may need changes)
        let mut entries = self.entries.borrow_mut();
        for component in entry.filepath.accumulative_components() {
            entries.remove(&(component, MergeStage::None));
        }
        Ok(())
    }

    /// removes collisions where there was originally a directory but was replaced by a file
    fn remove_dir_file_collisions(&self, index_entry: &BitIndexEntry) -> BitResult<()> {
        //? unsure which implementation is better
        // doesn't seem to be a nice way to remove a range of a btreemap
        // self.entries.retain(|(path, _), _| !path.starts_with(index_entry.filepath));
        let mut to_remove = vec![];
        let mut entries = self.entries.borrow_mut();
        for (&(path, stage), _) in entries.range((index_entry.filepath, MergeStage::None)..) {
            if !path.starts_with(index_entry.filepath) {
                break;
            }
            to_remove.push((path, stage));
        }
        for ref key in to_remove {
            entries.remove(key);
        }
        Ok(())
    }

    /// remove directory/file and file/directory collisions that are possible in the index
    fn remove_collisions(&self, entry: &BitIndexEntry) -> BitResult<()> {
        self.remove_file_dir_collisions(entry)?;
        self.remove_dir_file_collisions(entry)?;
        Ok(())
    }

    pub fn add(&self, pathspec: &Pathspec) -> BitResult<()> {
        let mut did_add = false;
        pathspec.match_worktree()?.for_each(|entry| {
            did_add = true;
            self.add_entry(entry)
        })?;
        ensure!(did_add, "no files added: pathspec `{}` did not match any files", pathspec);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.borrow().keys().any(|(_, stage)| stage.is_merging())
    }
}

struct TreeBuilder<'a, 'r> {
    index: &'a BitIndex<'r>,
    repo: &'a BitRepo,
    index_entries: Peekable<IndexIterator>,
    // count the number of blobs created (not subtrees)
    // should match the number of index entries
}

impl<'a, 'r> TreeBuilder<'a, 'r> {
    pub fn new(index: &'a BitIndex<'r>) -> Self {
        Self { index, repo: index.repo, index_entries: index.std_iter().peekable() }
    }

    fn build_tree(&mut self, current_index_dir: impl AsRef<Path>, depth: usize) -> BitResult<Tree> {
        let mut entries = BTreeSet::new();
        let current_index_dir = current_index_dir.as_ref();
        while let Some(next_entry) = self.index_entries.peek() {
            let &BitIndexEntry { mode, filepath, hash, .. } = next_entry;
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

impl Default for MergeStage {
    fn default() -> Self {
        Self::None
    }
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

impl BitIndexInner {
    fn parse_header(r: &mut dyn BufRead) -> BitResult<BitIndexHeader> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        assert_eq!(&signature, BIT_INDEX_HEADER_SIG);
        let version = r.read_u32()?;
        ensure!(version == 2, "Only index format v2 is supported");
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
}

impl Serialize for BitIndexHeader {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let Self { signature, version, entryc } = self;
        writer.write_all(signature)?;
        writer.write_all(&version.to_be_bytes())?;
        writer.write_all(&entryc.to_be_bytes())?;
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

impl Serialize for BitIndexInner {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let mut hash_writer = HashWriter::new_sha1(writer);

        let entries = self.entries.borrow();
        let header = BitIndexHeader {
            signature: *BIT_INDEX_HEADER_SIG,
            version: BIT_INDEX_VERSION,
            entryc: entries.len() as u32,
        };
        header.serialize(&mut hash_writer)?;

        for entry in entries.values() {
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

impl Deserialize for BitIndexInner {
    fn deserialize(r: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        // this impl currently is not ideal as it basically has to read it twice
        // although the second time is reading from memory so maybe its not that bad?
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

        let bit_index = Self::new(entries, extensions);

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
