mod index_entry;

use crate::diff::*;
use crate::error::BitResult;
use crate::hash::BIT_HASH_SIZE;
use crate::io::{HashWriter, ReadExt, WriteExt};
use crate::iter::BitEntryIterator;
use crate::lockfile::Lockfile;
use crate::obj::{FileMode, Oid, Tree, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::time::Timespec;
use crate::util;
pub use index_entry::*;
use itertools::Itertools;
use num_enum::TryFromPrimitive;
use sha1::Digest;
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

impl<'r> DerefMut for BitIndex<'r> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
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
    entries: BitIndexEntries,
    pub extensions: Vec<BitIndexExtension>,
}

impl BitIndexInner {
    pub fn new(entries: BitIndexEntries, extensions: Vec<BitIndexExtension>) -> Self {
        Self { entries, extensions }
    }
}

// impl<'a> IntoIterator for &'a BitIndexInner {
//     type IntoIter = Copied<Values<'a, (BitPath, MergeStage), BitIndexEntry>>;
//     type Item = BitIndexEntry;

//     fn into_iter(self) -> Self::IntoIter {
//         self.entries.values().copied()
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

    pub fn is_racy_entry(&self, worktree_entry: &BitIndexEntry) -> bool {
        // shouldn't strict equality be enough but libgit2 is `<=`
        // all index entries should have time `<=` the index file as
        // they are read before the index is written
        // all worktree entries that have been modified since the index has been written
        // clearly has mtime >= the index mtime.
        // so racily clean entries are the one's with mtime strictly equal to the index file's mtime
        self.mtime.map(|mtime| mtime == worktree_entry.mtime).unwrap_or(false)
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, mut entry: BitIndexEntry) -> BitResult<()> {
        self.remove_collisions(&entry)?;
        if entry.hash.is_unknown() {
            entry.hash = self.repo.hash_blob(entry.path)?;
        }
        self.entries.insert(entry.as_key(), entry);
        Ok(())
    }

    pub fn remove_entry(&mut self, entry: &BitIndexEntry) -> BitResult<()> {
        assert!(
            self.entries.remove(&entry.as_key()).is_some(),
            "tried to remove nonexistent entry `{:?}`",
            entry.as_key()
        );
        Ok(())
    }

    /// makes the index exactly match the working tree (removes, updates, and adds)
    pub fn add_all(&mut self) -> BitResult<()> {
        struct AddAll<'a, 'r> {
            index: &'a mut BitIndex<'r>,
        }

        impl<'a, 'r> Differ<'r> for AddAll<'a, 'r> {
            fn index_mut(&mut self) -> &mut BitIndex<'r> {
                self.index
            }

            fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_modified(&mut self, _old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
                self.index.remove_entry(old)
            }
        }
        let diff = self.diff_worktree(Pathspec::MATCH_ALL)?;
        diff.apply(&mut AddAll { index: self })
    }

    pub fn add(&mut self, pathspec: &Pathspec) -> BitResult<()> {
        let mut iter = pathspec.match_worktree(self.repo)?.peekable();
        ensure!(
            iter.peek()?.is_some(),
            "no files added: pathspec `{}` did not match any files",
            pathspec
        );
        iter.for_each(|entry| self.add_entry(entry))?;
        Ok(())
    }
}

type IndexIterator = impl Iterator<Item = BitIndexEntry> + Clone + std::fmt::Debug;

impl BitIndexInner {
    pub fn std_iter(&self) -> IndexIterator {
        // this is pretty nasty, but I'm uncertain of a better way to dissociate the lifetime of
        // `self` from the returned iterator
        self.entries.values().cloned().collect_vec().into_iter()
    }

    pub fn iter(&self) -> impl BitEntryIterator {
        fallible_iterator::convert(self.std_iter().map(Ok))
    }

    /// find entry by path
    pub fn find_entry(&self, path: BitPath, stage: MergeStage) -> Option<BitIndexEntry> {
        self.entries.get(&(path, stage)).copied()
    }

    /// removes collisions where there was originally a file but was replaced by a directory
    fn remove_file_dir_collisions(&mut self, entry: &BitIndexEntry) -> BitResult<()> {
        //? only removing entries with no merge stage (may need changes)
        for component in entry.path.accumulative_components() {
            self.entries.remove(&(component, MergeStage::None));
        }
        Ok(())
    }

    /// removes collisions where there was originally a directory but was replaced by a file
    fn remove_dir_file_collisions(&mut self, index_entry: &BitIndexEntry) -> BitResult<()> {
        //? unsure which implementation is better
        // doesn't seem to be a nice way to remove a range of a btreemap
        // self.entries.retain(|(path, _), _| !path.starts_with(index_entry.path));
        let mut to_remove = vec![];
        for (&(path, stage), _) in self.entries.range((index_entry.path, MergeStage::None)..) {
            if !path.starts_with(index_entry.path) {
                break;
            }
            to_remove.push((path, stage));
        }
        for ref key in to_remove {
            self.entries.remove(key);
        }
        Ok(())
    }

    /// remove directory/file and file/directory collisions that are possible in the index
    fn remove_collisions(&mut self, entry: &BitIndexEntry) -> BitResult<()> {
        self.remove_file_dir_collisions(entry)?;
        self.remove_dir_file_collisions(entry)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.keys().any(|(_, stage)| stage.is_merging())
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
            let &BitIndexEntry { mode, path: filepath, hash, .. } = next_entry;
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
    fn parse_header(r: &mut impl BufRead) -> BitResult<BitIndexHeader> {
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

        let header = BitIndexHeader {
            signature: *BIT_INDEX_HEADER_SIG,
            version: BIT_INDEX_VERSION,
            entryc: self.entries.len() as u32,
        };
        header.serialize(&mut hash_writer)?;

        for entry in self.entries.values() {
            entry.serialize(&mut hash_writer)?;
        }

        for extension in &self.extensions {
            extension.serialize(&mut hash_writer)?;
        }

        let hash = hash_writer.finalize_sha1_hash();
        hash_writer.write_bit_hash(&hash)?;
        Ok(())
    }
}

impl Deserialize for BitIndexInner {
    fn deserialize(r: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        // this impl currently is not ideal as it basically has to read it twice
        // although the second time is reading from memory so maybe its not that bad?
        // its a bit awkward to use hashreader to read the extensions because we don't
        // know how long the extensions are
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
        let actual_hash = Oid::from(hasher.finalize());
        let expected_hash = Oid::new(hash.try_into().unwrap());
        ensure_eq!(actual_hash, expected_hash, "corrupted index (bad hash)");

        Ok(bit_index)
    }
}

#[cfg(test)]
mod tests;
