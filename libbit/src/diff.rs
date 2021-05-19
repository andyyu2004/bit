//! the diff in this module refers to workspace diffs (e.g. tree-to-index, index-to-worktree, tree-to-tree diffs etc)

use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::{BitEntryIterator, TreeIter, TreeIterator};
use crate::obj::{Tree, TreeEntry};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::{FallibleIterator, Fuse, Peekable};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

pub trait Differ<'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r>;
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()>;
}

pub trait TreeDiffer<'r> {
    /// unmatched new entry
    fn on_created(&mut self, new: TreeEntry) -> BitResult<()>;
    /// called when two entries are matched (could possibly be the same entry)
    fn on_match(&mut self, old: TreeEntry, new: TreeEntry) -> BitResult<()>;
    /// unmatched old entry
    fn on_deleted(&mut self, old: TreeEntry) -> BitResult<()>;
}

pub trait DiffBuilder<'r>: Differ<'r> {
    /// the type of the resulting diff (returned by `Self::run_diff`)
    type Diff;
    fn run_diff(self) -> BitResult<Self::Diff>;
}

pub struct GenericDiffer<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    differ: &'d mut D,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
    pd: std::marker::PhantomData<&'r ()>,
}

#[derive(Debug)]
enum Changed {
    Yes,
    No,
    Maybe,
}

impl<'d, 'r, D, I, J> Differ<'r> for GenericDiffer<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitEntryIterator,
    J: BitEntryIterator,
{
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.differ.index_mut()
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        self.old_iter.next()?;
        self.differ.on_deleted(old)
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.new_iter.next()?;
        self.differ.on_created(new)
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        self.old_iter.next()?;
        self.new_iter.next()?;
        // if we are here then we know that the path and stage of the entries match
        // however, that does not mean that the file has not changed
        if self.differ.index_mut().has_changes(&old, &new)? {
            self.differ.on_modified(old, new)?;
        }
        Ok(())
    }
}

impl<'d, 'r, D, I, J> GenericDiffer<'d, 'r, D, I, J>
where
    D: Differ<'r>,
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
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(&new)) => self.on_created(new)?,
                (Some(&old), None) => self.on_deleted(old)?,
                (Some(&old), Some(&new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match old.cmp(&new) {
                        Ordering::Less => self.on_deleted(old)?,
                        Ordering::Equal => self.on_modified(old, new)?,
                        Ordering::Greater => self.on_created(new)?,
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

#[derive(Debug, Default)]
pub struct TreeDiff {
    pub new: Vec<TreeEntry>,
    pub modified: Vec<(TreeEntry, TreeEntry)>,
    pub deleted: Vec<TreeEntry>,
}

pub struct TreeDifferImpl<'r> {
    // need an inner type otherwise we can't use `GenericDiffer::new` as it takes self but also needs `a` and `b`
    repo: &'r BitRepo,
    a: TreeIter<'r>,
    b: TreeIter<'r>,
    diff: TreeDiff,
}

impl<'r> TreeDifferImpl<'r> {
    pub fn new(repo: &'r BitRepo, a: &Tree, b: &Tree) -> Self {
        let a = repo.tree_iter(a);
        let b = repo.tree_iter(b);
        Self { repo, a, b, diff: Default::default() }
    }
}

impl<'r> TreeDifferImpl<'r> {
    fn run_diff(mut self) -> BitResult<TreeDiff> {
        // TODO is identical to GenericDiffer::generic_diff
        // maybe there is a a good way to unify the two
        // the difference is that subtrees can be skipped in TreeDiffer
        // but Differ goes through everything as a flat list (as that is the natural representation of the index)
        // maybe can implement TreeDiffer for IndexIter somehow then everything can use TreeDiffer which would be nice
        loop {
            match (self.a.peek()?, self.b.peek()?) {
                (None, None) => break,
                (None, Some(new)) => self.on_created(new)?,
                (Some(old), None) => self.on_deleted(old)?,
                (Some(old), Some(new)) => match old.cmp(&new) {
                    Ordering::Less => self.on_deleted(old)?,
                    Ordering::Equal => self.on_match(old, new)?,
                    Ordering::Greater => self.on_created(new)?,
                },
            };
        }
        Ok(self.diff)
    }
}

impl<'r> TreeDiffer<'r> for TreeDifferImpl<'r> {
    fn on_created(&mut self, new: TreeEntry) -> BitResult<()> {
        //? no need to recurse at all I think?
        self.diff.new.push(new);
        self.b.over()?;
        Ok(())
    }

    fn on_match(&mut self, old: TreeEntry, new: TreeEntry) -> BitResult<()> {
        if old.hash == new.hash && old.mode.is_dir() && new.mode.is_dir() {
            // if hashes match and both are directories we can step over them
            self.a.over()?;
            self.b.over()?;
        } else {
            if old.hash != new.hash {
                self.diff.modified.push((old, new));
            }
            self.a.next()?;
            self.b.next()?;
        }

        Ok(())
    }

    fn on_deleted(&mut self, old: TreeEntry) -> BitResult<()> {
        self.diff.deleted.push(old);
        self.a.over()?;
        Ok(())
    }
}

impl BitRepo {
    pub fn diff_index_worktree(&self) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_worktree())
    }

    pub fn diff_head_index(&self) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_head())
    }

    pub fn diff_tree_to_tree(&self, a: &Tree, b: &Tree) -> BitResult<TreeDiff> {
        TreeDifferImpl::new(self, a, b).run_diff()
    }
}

impl<'r> BitIndex<'r> {
    pub fn diff_worktree(&mut self) -> BitResult<WorkspaceDiff> {
        IndexWorktreeDiffer::new(self).run_diff()
    }

    pub fn diff_head(&mut self) -> BitResult<WorkspaceDiff> {
        HeadIndexDiffer::new(self).run_diff()
    }
}

pub(crate) struct HeadIndexDiffer<'a, 'r> {
    repo: &'r BitRepo,
    index: &'a mut BitIndex<'r>,
    new: Vec<BitIndexEntry>,
    staged: Vec<(BitIndexEntry, BitIndexEntry)>,
    deleted: Vec<BitIndexEntry>,
}

impl<'a, 'r> HeadIndexDiffer<'a, 'r> {
    pub fn new(index: &'a mut BitIndex<'r>) -> Self {
        let repo = index.repo;
        Self {
            index,
            repo,
            new: Default::default(),
            staged: Default::default(),
            deleted: Default::default(),
        }
    }
}

impl<'a, 'r> DiffBuilder<'r> for HeadIndexDiffer<'a, 'r> {
    type Diff = WorkspaceDiff;

    fn run_diff(mut self) -> BitResult<WorkspaceDiff> {
        let repo = self.repo;
        let index_iter = self.index.iter();
        GenericDiffer::run(&mut self, repo.head_iter()?, index_iter)?;
        Ok(WorkspaceDiff { deleted: self.deleted, modified: self.staged, new: self.new })
    }
}

impl<'a, 'r> Differ<'r> for HeadIndexDiffer<'a, 'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.new.push(new))
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.path, new.path);
        Ok(self.staged.push((old, new)))
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(old))
    }
}

#[derive(Debug, Default)]
pub struct WorkspaceDiff {
    // could consider making these TreeEntries
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
    fn apply<'r, D: Differ<'r>>(&self, differ: &mut D) -> BitResult<()>;
}

impl Diff for WorkspaceDiff {
    fn apply<'r, D: Differ<'r>>(&self, differ: &mut D) -> BitResult<()> {
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

pub(crate) struct IndexWorktreeDiffer<'a, 'r> {
    repo: &'r BitRepo,
    index: &'a mut BitIndex<'r>,
    untracked: Vec<BitIndexEntry>,
    modified: Vec<(BitIndexEntry, BitIndexEntry)>,
    deleted: Vec<BitIndexEntry>,
    // directories that only contain untracked files
    _untracked_dirs: HashSet<BitPath>,
}

impl<'a, 'r> IndexWorktreeDiffer<'a, 'r> {
    pub fn new(index: &'a mut BitIndex<'r>) -> Self {
        let repo = index.repo;
        Self {
            index,
            repo,
            untracked: Default::default(),
            modified: Default::default(),
            deleted: Default::default(),
            _untracked_dirs: Default::default(),
        }
    }
}

impl<'a, 'r> DiffBuilder<'r> for IndexWorktreeDiffer<'a, 'r> {
    type Diff = WorkspaceDiff;

    fn run_diff(mut self) -> BitResult<WorkspaceDiff> {
        let repo = self.repo;
        let index_iter = self.index.iter();
        GenericDiffer::run(&mut self, index_iter, repo.worktree_iter()?)?;
        Ok(WorkspaceDiff { new: self.untracked, modified: self.modified, deleted: self.deleted })
    }
}

impl<'a, 'r> Differ<'r> for IndexWorktreeDiffer<'a, 'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.untracked.push(new);
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.path, new.path);
        Ok(self.modified.push((old, new)))
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(old))
    }
}

impl<'r> BitIndex<'r> {
    //? maybe the parameters to this function need to be less general
    //? and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
    /// determine's whether `new` is *definitely* different from `old`
    // (preferably without comparing hashes)
    pub fn has_changes(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        trace!("BitIndex::has_changes({} -> {})?", old.path, new.path);
        // should only be comparing the same file
        assert_eq!(old.path, new.path);
        // the "old" entry should always have a calculated hash
        assert!(old.hash.is_known());
        assert_eq!(old.stage(), MergeStage::None);

        match self.has_changes_inner(old, new)? {
            Changed::Yes => Ok(true),
            Changed::No => Ok(false),
            Changed::Maybe => {
                // this section should only be hit if `old` is an index entry
                // there are currently only two types of diffs, index-worktree and head-index,
                // where the left side is `old` and the right is `new`
                // so the `old` parameter is either a head entry or an index entry
                // a head_entry should never reach this section as it should always have a known hash
                // (from the TreeEntry). To assert this we just check the filepath to be empty
                // (as this is the default value given when a tree entry is converted to an index entry)
                assert!(!old.path.is_empty());

                // file may have changed, but we are not certain, so check the hash
                let mut new_hash = new.hash;
                if new_hash.is_unknown() {
                    new_hash = self.repo.hash_blob(new.path)?;
                }

                let changed = old.hash != new_hash;
                if !changed {
                    // update index entries so we don't hit this slow path again
                    // we just replace the old entry with the new one to do the update
                    // TODO add test for this
                    debug_assert_eq!(old.as_key(), new.as_key());
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
        if old.hash == new.hash {
            debug!("{} unchanged: hashes match {} {}", old.path, old.hash, new.hash);
            return Ok(Changed::No);
        } else if new.hash.is_known() {
            // asserted old.hash.is_known() in outer function
            debug!("{} changed: two known hashes don't match {} {}", old.path, old.hash, new.hash);
            return Ok(Changed::Yes);
        }

        if old.mtime == new.mtime {
            if self.is_racy_entry(old) {
                debug!("racy entry {}", new.path);
                return Ok(Changed::Maybe);
            }
            debug!("{} unchanged: non-racy mtime match {} {}", old.path, old.mtime, new.mtime);
            return Ok(Changed::No);
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

        if tls::with_config(|config| config.filemode())? && old.mode != new.mode {
            debug!("{} changed: filemode {} -> {}", old.path, old.mode, new.mode);
            return Ok(Changed::Yes);
        }

        debug!("{} uncertain if changed", old.path);

        Ok(Changed::Maybe)
    }
}

#[cfg(test)]
mod tests;
