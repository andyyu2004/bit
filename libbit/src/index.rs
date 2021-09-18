mod index_entry;
mod index_inner;
mod reuc;
mod tree_cache;

pub use self::tree_cache::BitTreeCache;
pub use index_entry::*;
pub use index_inner::{BitIndexInner, Conflict, ConflictType, Conflicts};

use self::reuc::BitReuc;
use crate::diff::*;
use crate::error::BitResult;
use crate::hash::OID_SIZE;
use crate::io::{HashWriter, ReadExt, WriteExt};
use crate::iter::{BitEntry, BitEntryIterator, BitTreeIterator, IndexTreeIter};
use crate::lockfile::Filelock;
use crate::obj::{FileMode, Oid, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::time::Timespec;
use bitflags::bitflags;
#[allow(unused_imports)]
use fallible_iterator::FallibleIterator;
use itertools::Itertools;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
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
pub struct BitIndex<'rcx> {
    pub repo: BitRepo<'rcx>,
    // index file may not yet exist
    mtime: Option<Timespec>,
    inner: Filelock<BitIndexInner>,
}

impl<'rcx> Deref for BitIndex<'rcx> {
    type Target = BitIndexInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'rcx> DerefMut for BitIndex<'rcx> {
    /// refer to note in [crate::lockfile::Filelock] `deref_mut`
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

trait BitIndexExt {
    fn signature(&self) -> [u8; 4];
}

impl<'rcx> BitIndex<'rcx> {
    pub fn rollback(&self) {
        self.inner.rollback()
    }

    pub fn new(repo: BitRepo<'rcx>) -> BitResult<Self> {
        let index_path = repo.index_path();
        let mtime = std::fs::metadata(index_path).as_ref().map(Timespec::mtime).ok();
        let inner = Filelock::lock(index_path)?;
        Ok(Self { repo, inner, mtime })
    }

    pub(crate) fn update_cache_tree(&mut self, tree: Oid) -> BitResult<()> {
        let repo = self.repo;
        match self.tree_cache.as_mut() {
            Some(tree_cache) => tree_cache.update(repo, tree)?,
            None => self.tree_cache = Some(BitTreeCache::read_tree(repo, tree)?),
        }
        Ok(())
    }

    /// Read a tree object into the index. The current contents of the index will be replaced.
    pub fn read_tree(&mut self, treeish: impl Treeish<'rcx>) -> BitResult<()> {
        let repo = self.repo;
        let tree = treeish.treeish_oid(repo)?;
        // TODO use the iterator diff API
        let diff = self.diff_tree(tree, Pathspec::MATCH_ALL)?;
        self.apply_diff(&diff)?;

        self.update_cache_tree(tree)?;

        // index should now exactly match the tree
        debug_assert!(self.diff_tree(tree, Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    }

    /// Builds a tree object from the current index entries and writes it and all subtrees
    /// to disk returning the oid of the root tree.
    pub fn write_tree(&mut self) -> BitResult<Oid> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }

        let tree_oid = self.index_tree_iter().build_tree(self.repo, self.tree_cache())?;
        // refresh the tree_cache using the tree we just built
        self.update_cache_tree(tree_oid)?;
        Ok(tree_oid)
    }

    pub(crate) fn virtual_write_tree(&mut self) -> BitResult<Oid> {
        self.repo.with_virtual_write(|| self.write_tree())
    }

    pub fn is_racy_entry(&self, worktree_entry: &BitIndexEntry) -> bool {
        // https://git-scm.com/docs/racy-git/en
        self.mtime.map(|mtime| mtime <= worktree_entry.mtime).unwrap_or(true)
    }

    fn add_entry_common(&mut self, mut entry: BitIndexEntry) -> BitResult<()> {
        self.remove_collisions(entry.path)?;
        entry.oid = entry.write(self.repo)?;
        debug_assert!(entry.oid.is_known());
        self.insert_entry(entry);
        Ok(())
    }

    pub fn add_entry_from_path(&mut self, path: &Path) -> BitResult<()> {
        self.add_entry(BitIndexEntry::from_absolute_path(self.repo, path)?)
    }

    /// Add fully populated index entry to the index. If entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, mut entry: BitIndexEntry) -> BitResult<()> {
        entry.fill(self.repo)?;
        self.remove_conflicted(entry.path);
        self.add_entry_common(entry)
    }

    pub fn add_conflicted_entry(
        &mut self,
        mut entry: BitIndexEntry,
        stage: MergeStage,
    ) -> BitResult<()> {
        assert!(entry.oid.is_known());
        self.remove_entry((entry.path, MergeStage::None));

        entry.set_stage(stage);
        self.add_entry_common(entry)?;
        Ok(())
    }

    pub(crate) fn unlink_and_remove_blob(
        &mut self,
        (path, stage): (BitPath, MergeStage),
    ) -> BitResult<()> {
        std::fs::remove_file(self.repo.to_absolute_path(&path))?;
        self.remove_entry((path, stage));
        Ok(())
    }

    pub(crate) fn write_and_add_blob(&mut self, entry: BitIndexEntry) -> BitResult<()> {
        debug_assert!(entry.oid.is_known());
        entry.write_to_disk(self.repo)?;
        self.add_entry(entry)
    }

    /// Makes the index exactly match the working tree (removes, updates, and adds)
    pub fn add_all(&mut self) -> BitResult<()> {
        // TODO use iterator diff api
        let diff = self.diff_worktree(Pathspec::MATCH_ALL)?;
        self.apply_diff(&diff)?;

        // worktree should exactly match the index after `add_all`
        debug_assert!(self.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    }

    fn apply_diff(&mut self, diff: &WorkspaceStatus) -> BitResult<()> {
        struct IndexApplier<'a, 'rcx> {
            index: &'a mut BitIndex<'rcx>,
        }

        impl<'a, 'rcx> Differ for IndexApplier<'a, 'rcx> {
            fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(new)
            }

            fn on_modified(&mut self, _old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(new)
            }

            fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
                self.index.remove_entry(old.key());
                Ok(())
            }
        }

        diff.apply_with(&mut IndexApplier { index: self })
    }

    pub fn add(&mut self, pathspec: &Pathspec) -> BitResult<()> {
        // TODO shouldn't need to iterate over entire worktree always
        // i.e. `bit add foo` we should be able to just start traversing from `foo` rather than
        // the entire worktree and then filter out things
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
    Base   = 1,
    Ours   = 2,
    Theirs = 3,
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
    pub fn is_unmerged(self) -> bool {
        self as u8 > 0
    }
}

impl Display for MergeStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
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

#[cfg(test)]
mod bench;
