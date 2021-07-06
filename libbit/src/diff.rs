//! the diff in this module refers to workspace diffs (e.g. tree-to-index, index-to-worktree, tree-to-tree diffs etc)

use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitEntryIterator, BitTreeIterator};
use crate::obj::{FileMode, Oid, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::time::Timespec;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;
use std::collections::HashSet;

pub trait Differ {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()>;
    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()>;
    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()>;
}

pub trait IndexDiffer<'rcx>: Differ {
    fn index_mut(&mut self) -> &mut BitIndex<'rcx>;
}

pub trait DiffBuilder<'rcx>: IndexDiffer<'rcx> {
    /// the type of the resulting diff (returned by `Self::build_diff`)
    type Diff;
    fn build_diff(self) -> BitResult<Self::Diff>;
}

pub trait TreeDiffer<'rcx> {
    /// unmatched new entry
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()>;
    /// called when two entries are matched (could possibly be the same entry)
    fn on_matched(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
    /// unmatched old entry
    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()>;
}

// TODO this is actually now specific to worktree index diffs
pub struct GenericDiffer<'d, 'rcx, D, I, J>
where
    D: IndexDiffer<'rcx>,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    differ: &'d mut D,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
    pd: std::marker::PhantomData<&'rcx ()>,
}

#[derive(Debug)]
enum Changed {
    Yes,
    No,
    Maybe,
}

impl<'d, 'rcx, D, I, J> GenericDiffer<'d, 'rcx, D, I, J>
where
    D: IndexDiffer<'rcx>,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    fn new(differ: &'d mut D, old_iter: I, new_iter: J) -> Self {
        Self {
            old_iter: old_iter.fuse().peekable(),
            new_iter: new_iter.fuse().peekable(),
            differ,
            pd: std::marker::PhantomData,
        }
    }

    pub fn diff_generic(&mut self) -> BitResult<()> {
        macro_rules! on_created {
            ($new:expr) => {{
                self.differ.on_created($new)?;
                self.new_iter.next()?;
            }};
        }

        macro_rules! on_deleted {
            ($old:expr) => {{
                self.differ.on_deleted($old)?;
                self.old_iter.next()?;
            }};
        }

        macro_rules! on_modified {
            ($old:expr => $new:expr) => {{
                if self.differ.index_mut().has_changes($old, $new)? {
                    self.differ.on_modified($old, $new)?;
                }
                self.old_iter.next()?;
                self.new_iter.next()?;
            }};
        }

        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(new)) => on_created!(new),
                (Some(old), None) => on_deleted!(old),
                (Some(old), Some(new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match diff_cmp(old, new) {
                        Ordering::Less => on_deleted!(old),
                        Ordering::Equal => on_modified!(old => new),
                        Ordering::Greater => on_created!(new),
                    }
                }
            };
        }

        Ok(())
    }

    pub fn run(differ: &'d mut D, old_iter: I, new_iter: J) -> BitResult<()> {
        Self::new(differ, old_iter, new_iter).diff_generic()
    }
}

pub struct TreeDifferGeneric<'rcx, I, J> {
    repo: BitRepo<'rcx>,
    old_iter: I,
    new_iter: J,
    diff: WorkspaceDiff,
}

impl<'rcx, I, J> TreeDifferGeneric<'rcx, I, J> {
    pub fn new(repo: BitRepo<'rcx>, old_iter: I, new_iter: J) -> Self {
        Self { repo, old_iter, new_iter, diff: Default::default() }
    }
}

// comparison function for differs
// cares about paths first, then modes second
fn diff_cmp(a: &impl BitEntry, b: &impl BitEntry) -> std::cmp::Ordering {
    a.sort_path().cmp(&b.sort_path()).then_with(|| a.mode().cmp(&b.mode()))
}

impl<'rcx, I, J> TreeDifferGeneric<'rcx, I, J>
where
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    fn build_diff(mut self) -> BitResult<WorkspaceDiff> {
        trace!("TreeDifferGeneric::build_diff");
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(new)) => self.on_created(new)?,
                (Some(old), None) => self.on_deleted(old)?,
                (Some(old), Some(new)) => match diff_cmp(&old, &new) {
                    Ordering::Less => self.on_deleted(old)?,
                    Ordering::Equal => self.on_matched(old, new)?,
                    Ordering::Greater => self.on_created(new)?,
                },
            };
        }
        Ok(self.diff)
    }
}

impl<'rcx, I, J> TreeDiffer<'rcx> for TreeDifferGeneric<'rcx, I, J>
where
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_created(new: {})", new.path());
        if new.is_tree() {
            self.new_iter.collect_over_tree(&mut self.diff.new)
        } else {
            self.diff.new.push(new);
            self.new_iter.next()?;
            Ok(())
        }
    }

    fn on_matched(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_match(path: {})", new.path());
        // one of the oid's may be unknown due to being a pseudotree
        debug_assert!(old.oid().is_known() || new.oid().is_known());
        match (old.mode(), new.mode()) {
            (FileMode::TREE, FileMode::TREE) if old == new => {
                // if hashes match and both are directories we can step over them
                self.old_iter.over()?;
                self.new_iter.over()?;
            }
            (FileMode::TREE, FileMode::TREE) => {
                // two trees with non matching oids, then step inside for both
                self.old_iter.next()?;
                self.new_iter.next()?;
            }
            _ if old.is_file() && new.is_file() && old.oid() == new.oid() => {
                // matching files
                debug_assert!(old.oid().is_known() && new.oid().is_known());
                self.old_iter.next()?;
                self.new_iter.next()?;
            }
            _ => {
                debug_assert!(
                    old.is_file() && new.is_file(),
                    "A tree vs nontree should not call `on_match`, check the ordering function.
                    If two entries have the same path then files should come before directories (as per `diff_cmp`).
                    We do not currently detect type changes, and instead treat this as an add/remove pair"
                );
                // non matching files
                self.diff.modified.push((old, new));
                self.old_iter.next()?;
                self.new_iter.next()?;
            }
        };

        Ok(())
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_deleted(old: {})", old.path());
        if old.is_tree() {
            self.old_iter.collect_over_tree(&mut self.diff.deleted)
        } else {
            self.diff.deleted.push(old);
            self.old_iter.next()?;
            Ok(())
        }
    }
}

impl<'rcx> BitRepo<'rcx> {
    /// diff the tree belonging to the commit pointed to by `reference` with the index
    pub fn diff_ref_index(self, reference: BitRef, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        let commit_oid = self.try_fully_resolve_ref(reference)?;
        let tree_oid = match commit_oid {
            Some(oid) => self.read_obj(oid)?.into_commit().tree(),
            None => Oid::UNKNOWN,
        };
        self.diff_tree_index(tree_oid, pathspec)
    }

    pub fn diff_tree_index(self, tree: Oid, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_tree(tree, pathspec))
    }

    pub fn diff_index_worktree(self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_worktree(pathspec))
    }

    pub fn diff_head_index(self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_head(pathspec))
    }

    pub fn diff_tree_to_tree(self, a: Oid, b: Oid) -> BitResult<WorkspaceDiff> {
        TreeDifferGeneric::new(self, self.tree_iter(a), self.tree_iter(b)).build_diff()
    }
}

impl<'rcx> BitIndex<'rcx> {
    pub fn diff_worktree(&mut self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        IndexWorktreeDiffer::new(self, pathspec).build_diff()
    }

    pub fn diff_tree(&mut self, tree: Oid, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        let tree_iter = pathspec.match_tree_iter(self.repo.tree_iter(tree));
        let index_iter = pathspec.match_tree_iter(self.tree_iter());
        TreeDifferGeneric::new(self.repo, tree_iter, index_iter).build_diff()
    }

    pub fn diff_head(&mut self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.diff_tree(self.repo.head_tree()?, pathspec)
    }
}

#[derive(Debug, Default, PartialEq)]
// tree entries here are not sufficient as we use this to manipulate the index which requires
// some data in IndexEntries that are not present in TreeEntries (e.g. stage)
// invariants:
// should not contain directories: directories should be expanded first before insertion
pub struct WorkspaceDiff {
    pub new: Vec<BitIndexEntry>,
    pub modified: Vec<(BitIndexEntry, BitIndexEntry)>,
    pub deleted: Vec<BitIndexEntry>,
}

impl WorkspaceDiff {
    pub fn is_empty(&self) -> bool {
        self.new.is_empty() && self.deleted.is_empty() && self.modified.is_empty()
    }
}

pub trait Diff {
    fn apply_with<D: Differ>(&self, differ: &mut D) -> BitResult<()>;
}

impl Diff for WorkspaceDiff {
    fn apply_with<D: Differ>(&self, differ: &mut D) -> BitResult<()> {
        for deleted in self.deleted.iter() {
            differ.on_deleted(deleted)?;
        }
        for (old, new) in self.modified.iter() {
            differ.on_modified(old, new)?;
        }
        for new in self.new.iter() {
            differ.on_created(new)?;
        }
        Ok(())
    }
}

pub(crate) struct IndexWorktreeDiffer<'a, 'rcx> {
    index: &'a mut BitIndex<'rcx>,
    pathspec: Pathspec,
    diff: WorkspaceDiff,
    // directories that only contain untracked files
    _untracked_dirs: HashSet<BitPath>,
}

impl<'a, 'rcx> IndexWorktreeDiffer<'a, 'rcx> {
    pub fn new(index: &'a mut BitIndex<'rcx>, pathspec: Pathspec) -> Self {
        Self { index, pathspec, diff: Default::default(), _untracked_dirs: Default::default() }
    }
}

impl<'a, 'rcx> DiffBuilder<'rcx> for IndexWorktreeDiffer<'a, 'rcx> {
    type Diff = WorkspaceDiff;

    fn build_diff(mut self) -> BitResult<WorkspaceDiff> {
        let index_iter = self.pathspec.match_index(self.index)?;
        let worktree_iter = self.pathspec.match_worktree(self.index)?;
        GenericDiffer::run(&mut self, index_iter, worktree_iter)?;
        Ok(self.diff)
    }
}

impl<'a, 'rcx> Differ for IndexWorktreeDiffer<'a, 'rcx> {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
        self.diff.new.push(*new);
        Ok(())
    }

    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
        debug_assert_eq!(old.path, new.path);
        self.diff.modified.push((*old, *new));
        Ok(())
    }

    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
        self.diff.deleted.push(*old);
        Ok(())
    }
}

impl<'a, 'rcx> IndexDiffer<'rcx> for IndexWorktreeDiffer<'a, 'rcx> {
    fn index_mut(&mut self) -> &mut BitIndex<'rcx> {
        self.index
    }
}

impl<'rcx> BitIndex<'rcx> {
    //? maybe the parameters to this function need to be less general
    //? and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
    /// determine's whether `new` is *definitely* different from `old`
    // (preferably without comparing hashes)
    pub fn has_changes(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        trace!("BitIndex::has_changes({} -> {})?", old.path, new.path);
        // should only be comparing the same file
        debug_assert_eq!(old.path, new.path);
        // the "old" entry should always have a calculated hash
        debug_assert!(old.oid.is_known());
        debug_assert_eq!(old.stage(), MergeStage::None);

        match self.has_changes_inner(old, new)? {
            Changed::Yes => Ok(true),
            Changed::No => Ok(false),
            Changed::Maybe => {
                // this section should only be hit if `old` is an index entry
                // there are currently only two types of diffs, index-worktree and head-index,
                // where the left side is `old` and the right is `new`
                // so the `old` parameter is either a head entry or an index entry
                // A tree_entry should never reach this section as it should always have a known hash
                // (from the TreeEntry). To assert this we just check the ctime to be zero
                // (as this is the default value given when a tree entry is converted to an index entry and would not be possible otherwise)
                debug_assert!(old.ctime != Timespec::ZERO);

                // file may have changed, but we are not certain, so check the hash
                let mut new_hash = new.oid;
                if new_hash.is_unknown() {
                    new_hash = self.repo.hash_blob(new.path)?;
                }

                let changed = old.oid != new_hash;
                if !changed {
                    // update index entries so we don't hit this slow path again
                    // we just replace the old entry with the new one to do the update
                    // TODO add test for this
                    debug_assert_eq!(old.key(), new.key());
                    self.add_entry(*new)?;
                }
                Ok(changed)
            }
        }
    }

    /// determines whether two index_entries are definitely different
    /// `new` should be the "old" entry, and `other` should be the "new" one
    fn has_changes_inner(&self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<Changed> {
        //? check assume_unchanged and skip_worktree here?

        // we must check the hash before anything else in case the entry is generated from a `TreeEntry`
        // where most of the fields are zeroed but the hash is known
        // these checks confirm whether entries have definitely NOT changed
        if old.oid == new.oid {
            debug!("{} unchanged: hashes match {} {}", old.path, old.oid, new.oid);
            return Ok(Changed::No);
        } else if new.oid.is_known() {
            // asserted old.hash.is_known() in outer function
            debug!("{} changed: two known hashes don't match {} {}", old.path, old.oid, new.oid);
            return Ok(Changed::Yes);
        }

        if old.mtime == new.mtime {
            if self.is_racy_entry(old) {
                // don't return immediately, check other stats too to see if we can detect a change
                debug!("racy entry {}", new.path);
            } else {
                debug!("{} unchanged: non-racy mtime match {} {}", old.path, old.mtime, new.mtime);
                return Ok(Changed::No);
            }
        }

        // these checks confirm if the entry definitely have changed
        // could probably add in a few of the other fields but not that important?

        if old.filesize != new.filesize {
            debug!("{} changed: filesize {} -> {}", old.path, old.filesize, new.filesize);
            return Ok(Changed::Yes);
        }

        if old.inode != new.inode {
            debug!("{} changed: inode {} -> {}", old.path, old.inode, new.inode);
            return Ok(Changed::Yes);
        }

        if self.repo.config().filemode()? && old.mode != new.mode {
            debug!("{} changed: filemode {} -> {}", old.path, old.mode, new.mode);
            return Ok(Changed::Yes);
        }

        debug!("{} uncertain if changed", old.path);

        Ok(Changed::Maybe)
    }
}

#[cfg(test)]
mod tests;
