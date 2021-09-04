mod tree_diff;
pub use tree_diff::*;

use crate::error::{BitGenericError, BitResult};
use crate::index::{BitIndex, BitIndexEntry};
use crate::iter::{BitEntry, BitEntryIterator, BitTreeIterator};
use crate::obj::{Oid, Treeish};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use crate::time::Timespec;
use fallible_iterator::{FallibleIterator, Fuse, Peekable};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DiffEntry {
    Deleted(BitIndexEntry),
    Modified(BitIndexEntry, BitIndexEntry),
    Created(BitIndexEntry),
}

pub struct IndexWorktreeDiffIter<'a, 'rcx, I, J>
where
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    index: &'a mut BitIndex<'rcx>,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
}

impl<'a, 'rcx, I, J> FallibleIterator for IndexWorktreeDiffIter<'a, 'rcx, I, J>
where
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    type Error = BitGenericError;
    type Item = DiffEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        macro_rules! on_created {
            ($new:expr) => {{
                let new = *$new;
                self.new_iter.next()?;
                return Ok(Some(DiffEntry::Created(new)));
            }};
        }

        macro_rules! on_deleted {
            ($old:expr) => {{
                let old = *$old;
                self.old_iter.next()?;
                return Ok(Some(DiffEntry::Deleted(old)));
            }};
        }

        macro_rules! on_modified {
            ($old:expr => $new:expr) => {{
                // it's written in this rather convoluted way to avoid
                // copying the index entry when unnecessary
                if self.index.has_changes($old, $new)? {
                    let old = *$old;
                    let new = *$new;
                    self.old_iter.next()?;
                    self.new_iter.next()?;
                    return Ok(Some(DiffEntry::Modified(old, new)));
                } else {
                    self.old_iter.next()?;
                    self.new_iter.next()?;
                }
            }};
        }

        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => return Ok(None),
                (None, Some(new)) => on_created!(new),
                (Some(old), None) => on_deleted!(old),
                (Some(old), Some(new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match old.entry_cmp(new) {
                        Ordering::Less => on_deleted!(old),
                        Ordering::Equal => on_modified!(old => new),
                        Ordering::Greater => on_created!(new),
                    }
                }
            };
        }
    }
}

pub trait Differ {
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()>;
}

pub struct IndexWorktreeDiffDriver<'d, 'rcx, D, I, J>
where
    D: Differ,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    differ: &'d mut D,
    index: &'d mut BitIndex<'rcx>,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
}

#[derive(Debug)]
enum Changed {
    Yes,
    No,
    Maybe,
}

impl<'d, 'rcx, D, I, J> IndexWorktreeDiffDriver<'d, 'rcx, D, I, J>
where
    D: Differ,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    fn new(differ: &'d mut D, index: &'d mut BitIndex<'rcx>, old_iter: I, new_iter: J) -> Self {
        Self {
            differ,
            index,
            old_iter: old_iter.fuse().peekable(),
            new_iter: new_iter.fuse().peekable(),
        }
    }

    pub fn run_diff(mut self) -> BitResult<()> {
        let differ = &mut self.differ;
        IndexWorktreeDiffIter {
            index: &mut self.index,
            old_iter: self.old_iter,
            new_iter: self.new_iter,
        }
        .for_each(|diff_entry| match diff_entry {
            DiffEntry::Deleted(old) => differ.on_deleted(old),
            DiffEntry::Modified(old, new) => differ.on_modified(old, new),
            DiffEntry::Created(new) => differ.on_created(new),
        })
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn diff_treeish_index(
        self,
        treeish: impl Treeish<'rcx>,
        pathspec: Pathspec,
    ) -> BitResult<WorkspaceStatus> {
        self.diff_tree_index(treeish.treeish_oid(self)?, pathspec)
    }

    pub fn diff_ref_index(
        self,
        reference: BitRef,
        pathspec: Pathspec,
    ) -> BitResult<WorkspaceStatus> {
        // we need to ensure diff still works even if HEAD points to a nonexistent branch
        // in which case we just do a diff against an empty tree
        let tree_oid = match self.resolve_ref(reference)? {
            BitRef::Direct(oid) => oid.treeish_oid(self)?,
            BitRef::Symbolic(..) => Oid::UNKNOWN,
        };
        self.diff_tree_index(tree_oid, pathspec)
    }

    /// diff the tree belonging to the commit pointed to by `reference` with the index
    pub fn diff_rev_index(self, rev: &Revspec, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        let reference = self.resolve_rev(rev)?;
        self.diff_ref_index(reference, pathspec)
    }

    pub fn diff_tree_worktree(self, tree: Oid, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        self.with_index(|index| {
            let tree_iter = pathspec.match_tree_iter(self.tree_iter(tree));
            let worktree_iter = pathspec.match_tree_iter(index.worktree_tree_iter()?);
            self.diff_iterators(tree_iter, worktree_iter)
        })
    }

    pub fn diff_tree_index(self, tree: Oid, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        self.with_index_mut(|index| index.diff_tree(tree, pathspec))
    }

    pub fn diff_index_worktree(self, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        self.with_index_mut(|index| index.diff_worktree(pathspec))
    }

    pub fn diff_head_index(self, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        self.with_index_mut(|index| index.diff_head(pathspec))
    }

    pub fn diff_tree_to_tree(self, a: Oid, b: Oid) -> BitResult<WorkspaceStatus> {
        self.diff_iterators(self.tree_iter(a), self.tree_iter(b))
    }

    pub fn diff_tree_to_tree_with_opts(
        self,
        a: Oid,
        b: Oid,
        opts: DiffOpts,
    ) -> BitResult<WorkspaceStatus> {
        self.diff_iterators_with_opts(self.tree_iter(a), self.tree_iter(b), opts)
    }

    pub fn tree_diff_iter<I: BitTreeIterator, J: BitTreeIterator>(
        self,
        a: I,
        b: J,
    ) -> TreeDiffIter<'rcx, I, J> {
        self.tree_diff_iter_with_opts(a, b, Default::default())
    }

    pub fn tree_diff_iter_with_opts<I, J>(
        self,
        a: I,
        b: J,
        opts: DiffOpts,
    ) -> TreeDiffIter<'rcx, I, J>
    where
        I: BitTreeIterator,
        J: BitTreeIterator,
    {
        TreeDiffIter::new(self, a, b, opts)
    }

    pub fn diff_iterators_with_opts(
        self,
        a: impl BitTreeIterator,
        b: impl BitTreeIterator,
        opts: DiffOpts,
    ) -> BitResult<WorkspaceStatus> {
        TreeStatusDiffer::default().build_diff(self, a, b, opts)
    }

    pub fn diff_iterators(
        self,
        a: impl BitTreeIterator,
        b: impl BitTreeIterator,
    ) -> BitResult<WorkspaceStatus> {
        self.diff_iterators_with_opts(a, b, DiffOpts::default())
    }
}

impl<'rcx> BitIndex<'rcx> {
    pub fn diff_worktree(&mut self, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        IndexWorktreeDiffer::new(pathspec).build_diff(self)
    }

    pub fn diff_tree(&self, tree: Oid, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        let tree_iter = self.repo.tree_iter(tree);
        self.diff_iterator(tree_iter, pathspec)
    }

    pub fn diff_iterator(
        &self,
        tree_iter: impl BitTreeIterator,
        pathspec: Pathspec,
    ) -> BitResult<WorkspaceStatus> {
        let tree_iter = pathspec.match_tree_iter(tree_iter);
        let index_iter = pathspec.match_tree_iter(self.index_tree_iter());
        self.repo.diff_iterators(tree_iter, index_iter)
    }

    pub fn diff_head(&self, pathspec: Pathspec) -> BitResult<WorkspaceStatus> {
        self.diff_tree(self.repo.head_tree()?, pathspec)
    }
}

#[derive(Debug, Default, PartialEq)]
// tree entries here are not sufficient as we use this to manipulate the index which requires
// some data in IndexEntries that are not present in TreeEntries (e.g. stage)
// invariants:
// should not contain directories: directories should be expanded first before insertion
pub struct WorkspaceStatus {
    pub new: Vec<BitIndexEntry>,
    pub modified: Vec<(BitIndexEntry, BitIndexEntry)>,
    pub deleted: Vec<BitIndexEntry>,
}

impl WorkspaceStatus {
    pub fn len(&self) -> usize {
        self.new.len() + self.modified.len() + self.deleted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.new.is_empty() && self.deleted.is_empty() && self.modified.is_empty()
    }
}

pub trait Diff {
    fn apply_with<D: Differ>(&self, differ: &mut D) -> BitResult<()>;
}

impl Diff for WorkspaceStatus {
    fn apply_with<D: Differ>(&self, differ: &mut D) -> BitResult<()> {
        for &deleted in self.deleted.iter() {
            differ.on_deleted(deleted)?;
        }
        for &(old, new) in self.modified.iter() {
            differ.on_modified(old, new)?;
        }
        for &new in self.new.iter() {
            differ.on_created(new)?;
        }
        Ok(())
    }
}

pub(crate) struct IndexWorktreeDiffer {
    pathspec: Pathspec,
    status: WorkspaceStatus,
    // directories that only contain untracked files
    _untracked_dirs: HashSet<BitPath>,
}

impl IndexWorktreeDiffer {
    pub fn new(pathspec: Pathspec) -> Self {
        Self { pathspec, status: Default::default(), _untracked_dirs: Default::default() }
    }

    fn build_diff(mut self, index: &mut BitIndex<'_>) -> BitResult<WorkspaceStatus> {
        let index_iter = self.pathspec.match_index(index)?;
        let worktree_iter = self.pathspec.match_worktree(index)?;
        IndexWorktreeDiffDriver::new(&mut self, index, index_iter, worktree_iter).run_diff()?;
        Ok(self.status)
    }
}

impl Differ for IndexWorktreeDiffer {
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.status.new.push(new);
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        debug_assert_eq!(old.path, new.path);
        self.status.modified.push((old, new));
        Ok(())
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        self.status.deleted.push(old);
        Ok(())
    }
}

impl<'rcx> BitIndex<'rcx> {
    pub fn is_worktree_entry_modified(
        &mut self,
        worktree_entry: &BitIndexEntry,
    ) -> BitResult<bool> {
        match self.find_entry(worktree_entry.key()) {
            Some(&index_entry) => self.has_changes(&index_entry, worktree_entry),
            None => Ok(true),
        }
    }

    //? maybe the parameters to this function need to be less general
    //? and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
    /// determine's whether `new` is *definitely* different from `old`
    // (preferably without comparing hashes)
    fn has_changes(
        &mut self,
        index_entry: &BitIndexEntry,
        worktree_entry: &BitIndexEntry,
    ) -> BitResult<bool> {
        trace!("BitIndex::has_changes({} -> {})?", index_entry.path, worktree_entry.path);
        // should only be comparing the same file
        debug_assert_eq!(index_entry.path, worktree_entry.path);
        // the "old" entry should always have a calculated hash
        debug_assert!(index_entry.oid.is_known());

        match self.has_changes_inner(index_entry, worktree_entry)? {
            Changed::Yes => Ok(true),
            Changed::No => Ok(false),
            Changed::Maybe => {
                // This section should only be hit if `old` is an index entry
                // A "tree_entry" should never reach this section as it should always have a known hash.
                // To assert this we just check the ctime to be zero.
                // (as this is the default value given when a tree entry is converted to an index entry and would not be possible otherwise)
                debug_assert!(index_entry.ctime != Timespec::ZERO);

                // file may have changed, but we are not certain, so check the hash
                let mut new_hash = worktree_entry.oid;
                if new_hash.is_unknown() {
                    new_hash = self.repo.hash_blob_from_worktree(worktree_entry.path)?;
                }

                let changed = index_entry.oid != new_hash;
                if !changed {
                    // update index entries so we don't hit this slow path again
                    // we just replace the old entry with the new one to do the update
                    // TODO add test for this
                    debug_assert_eq!(index_entry.key(), worktree_entry.key());
                    self.add_entry(*worktree_entry)?;
                }
                Ok(changed)
            }
        }
    }

    fn has_changes_inner(&self, idxe: &BitIndexEntry, wte: &BitIndexEntry) -> BitResult<Changed> {
        //? check assume_unchanged and skip_worktree here?

        // we must check the hash before anything else in case the entry is generated from a `TreeEntry`
        // where most of the fields are zeroed but the hash is known
        // these checks confirm whether entries have definitely NOT changed
        if idxe.oid == wte.oid {
            debug!("{} unchanged: hashes match {} {}", idxe.path, idxe.oid, wte.oid);
            return Ok(Changed::No);
        } else if wte.oid.is_known() {
            // asserted old.hash.is_known() in outer function
            debug!("{} changed: two known hashes don't match {} {}", idxe.path, idxe.oid, wte.oid);
            return Ok(Changed::Yes);
        }

        if idxe.mtime == wte.mtime {
            if self.is_racy_entry(idxe) {
                // don't return immediately, check other stats too to see if we can detect a change
                debug!("racy entry {}", wte.path);
            } else {
                debug!(
                    "{} unchanged: non-racy mtime match {} {}",
                    idxe.path, idxe.mtime, wte.mtime
                );
                return Ok(Changed::No);
            }
        }

        // these checks confirm if the entry definitely have changed
        // could probably add in a few of the other fields but not that important?
        if idxe.filesize != wte.filesize {
            debug!("{} changed: filesize {} -> {}", idxe.path, idxe.filesize, wte.filesize);
            return Ok(Changed::Yes);
        }

        if idxe.inode != wte.inode {
            debug!("{} changed: inode {} -> {}", idxe.path, idxe.inode, wte.inode);
            return Ok(Changed::Yes);
        }

        if self.repo.config().filemode()? && idxe.mode != wte.mode {
            debug!("{} changed: filemode {} -> {}", idxe.path, idxe.mode, wte.mode);
            return Ok(Changed::Yes);
        }

        debug!("{} uncertain if changed", idxe.path);

        Ok(Changed::Maybe)
    }
}

#[cfg(test)]
mod tests;
