mod index_entry;
mod index_inner;
mod reuc;
mod tree_cache;

use self::index_inner::BitIndexInner;
use self::reuc::BitReuc;
use self::tree_cache::BitTreeCache;
use crate::diff::*;
use crate::error::BitResult;
use crate::hash::BIT_HASH_SIZE;
use crate::io::{HashWriter, ReadExt, WriteExt};
use crate::iter::{BitEntryIterator, BitTreeIterator, IndexTreeIter};
use crate::lockfile::{Filelock};
use crate::obj::{BitObj, FileMode, Oid, Tree, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::time::Timespec;
use bitflags::bitflags;
pub use index_entry::*;
use itertools::Itertools;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
use std::iter::Peekable;
use std::ops::{Deref, DerefMut};
use std::path::Path;

const BIT_INDEX_HEADER_SIG: &[u8; 4] = b"DIRC";
const BIT_INDEX_TREECACHE_SIG: &[u8; 4] = b"TREE";
const BIT_INDEX_REUC_SIG: &[u8; 4] = b"REUC";
const BIT_INDEX_VERSION: u32 = 2;

bitflags! {
    pub struct BitIndexFlags: u8 {
        const DIRTY = 1 << 0;
    }
}

#[derive(Debug)]
pub struct BitIndex<'r> {
    pub repo: BitRepo<'r>,
    // index file may not yet exist
    mtime: Option<Timespec>,
    inner: Filelock<BitIndexInner>,
}

impl<'r> Deref for BitIndex<'r> {
    type Target = BitIndexInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'r> DerefMut for BitIndex<'r> {
    /// refer to note in [crate::lockfile::Filelock] `deref_mut`
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

trait BitIndexExt {
    fn signature(&self) -> [u8; 4];
}

impl<'r> BitIndex<'r> {
    pub fn rollback(&mut self) {
        self.inner.rollback()
    }

    pub fn new(repo: BitRepo<'r>) -> BitResult<Self> {
        let index_path = repo.index_path();
        let mtime = std::fs::metadata(index_path).as_ref().map(Timespec::mtime).ok();
        let inner = Filelock::lock(index_path)?;
        Ok(Self { repo, inner, mtime })
    }

    pub fn tree_iter(&self) -> IndexTreeIter<'_, 'r> {
        IndexTreeIter::new(self)
    }

    /// builds a tree object from the current index entries and writes it and all subtrees to disk
    pub fn write_tree(&self) -> BitResult<Tree> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }
        TreeBuilder::new(self).build()
    }

    pub fn is_racy_entry(&self, worktree_entry: &BitIndexEntry) -> bool {
        // https://git-scm.com/docs/racy-git/en
        self.mtime.map(|mtime| mtime <= worktree_entry.mtime).unwrap_or(true)
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, mut entry: BitIndexEntry) -> BitResult<()> {
        self.remove_collisions(&entry)?;

        let blob = self.repo.write_blob(entry.path)?;
        entry.oid = blob.oid();
        assert!(entry.oid.is_known());
        self.insert_entry(entry);
        Ok(())
    }

    /// makes the index exactly match the working tree (removes, updates, and adds)
    pub fn add_all(&mut self) -> BitResult<()> {
        struct AddAll<'a, 'r> {
            index: &'a mut BitIndex<'r>,
        }

        impl<'a, 'r> Apply for AddAll<'a, 'r> {
            fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_modified(&mut self, _old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
                Ok(self.index.remove_entry(old.key()))
            }
        }
        let diff = self.diff_worktree(Pathspec::MATCH_ALL)?;
        diff.apply(&mut AddAll { index: self })?;

        // worktree should exactly match the index after `add_all`
        debug_assert!(self.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    }

    pub fn add(&mut self, pathspec: &Pathspec) -> BitResult<()> {
        let mut iter = pathspec.match_worktree(self)?.peekable();
        // if a `match_all` doesn't match any files then it's not an error, just means there are no files
        ensure!(
            iter.peek()?.is_some() || pathspec.is_match_all(),
            "no files added: pathspec `{}` did not match any files",
            pathspec
        );
        iter.for_each(|entry| self.add_entry(entry))?;
        Ok(())
    }
}

type IndexStdIterator = impl Iterator<Item = BitIndexEntry> + Clone + std::fmt::Debug;
pub type IndexEntryIterator = impl BitEntryIterator;

struct TreeBuilder<'a, 'r> {
    index: &'a BitIndex<'r>,
    repo: BitRepo<'r>,
    index_entries: Peekable<IndexStdIterator>,
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
            let &BitIndexEntry { mode, path, oid, .. } = next_entry;
            // if the depth is greater than the number of components in the filepath
            // then we need to `break` and go out one level
            let (curr_dir, segment) = match path.try_split_path_at(depth) {
                Some(x) => x,
                None => break,
            };

            if curr_dir.as_path() != current_index_dir {
                break;
            }

            let nxt_path = curr_dir.as_path().join(segment);
            if nxt_path == path.as_path() {
                // only keep the final segment of the path inside the tree entry
                assert!(entries.insert(TreeEntry { mode, path: segment, oid }));
                self.index_entries.next();
            } else {
                let subtree = self.build_tree(&nxt_path, 1 + depth)?;
                assert!(entries.insert(TreeEntry {
                    path: segment,
                    mode: FileMode::DIR,
                    oid: subtree.oid()
                }));
            }
        }

        let tree = Tree::new(entries);
        self.repo.write_obj(&tree)?;
        Ok(tree)
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

#[cfg(test)]
impl quickcheck::Arbitrary for MergeStage {
    fn arbitrary(_g: &mut quickcheck::Gen) -> Self {
        Self::None
    }
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

    fn parse_extensions(mut buf: &[u8]) -> BitResult<HashMap<[u8; 4], BitIndexExtension>> {
        let mut extensions = HashMap::new();
        while buf.len() > BIT_HASH_SIZE {
            let signature: [u8; 4] = buf[0..4].try_into().unwrap();
            let size = u32::from_be_bytes(buf[4..8].try_into().unwrap());
            let data = buf[8..8 + size as usize].to_vec();
            extensions.insert(signature, BitIndexExtension { signature, size, data });
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tree_cache_tests;
