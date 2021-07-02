mod index_entry;
mod index_inner;
mod reuc;
mod tree_cache;

pub use index_entry::*;
pub use index_inner::BitIndexInner;

use self::reuc::BitReuc;
use self::tree_cache::BitTreeCache;
use crate::diff::*;
use crate::error::BitResult;
use crate::hash::OID_SIZE;
use crate::io::{HashWriter, ReadExt, WriteExt};
use crate::iter::{BitEntryIterator, BitTreeIterator, IndexTreeIter};
use crate::lockfile::Filelock;
use crate::obj::{FileMode, Oid, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::time::Timespec;
use bitflags::bitflags;
use itertools::Itertools;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
use std::ops::{Deref, DerefMut};

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
    pub fn rollback(&mut self) {
        self.inner.rollback()
    }

    pub fn new(repo: BitRepo<'rcx>) -> BitResult<Self> {
        let index_path = repo.index_path();
        let mtime = std::fs::metadata(index_path).as_ref().map(Timespec::mtime).ok();
        let inner = Filelock::lock(index_path)?;
        Ok(Self { repo, inner, mtime })
    }

    /// builds a tree object from the current index entries and writes it and all subtrees to disk
    pub fn write_tree(&self) -> BitResult<Oid> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }

        self.tree_iter().build_tree(self.repo)
    }

    pub fn is_racy_entry(&self, worktree_entry: &BitIndexEntry) -> bool {
        // https://git-scm.com/docs/racy-git/en
        self.mtime.map(|mtime| mtime <= worktree_entry.mtime).unwrap_or(true)
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, mut entry: BitIndexEntry) -> BitResult<()> {
        self.remove_collisions(&entry)?;

        entry.oid = self.repo.write_blob(entry.path)?;
        assert!(entry.oid.is_known());
        self.insert_entry(entry);
        Ok(())
    }

    /// makes the index exactly match the working tree (removes, updates, and adds)
    pub fn add_all(&mut self) -> BitResult<()> {
        struct AddAll<'a, 'rcx> {
            index: &'a mut BitIndex<'rcx>,
        }

        impl<'a, 'rcx> Apply for AddAll<'a, 'rcx> {
            fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_modified(&mut self, _old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
                self.index.add_entry(*new)
            }

            fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
                self.index.remove_entry(old.key());
                Ok(())
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
