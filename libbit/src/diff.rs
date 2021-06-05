//! the diff in this module refers to workspace diffs (e.g. tree-to-index, index-to-worktree, tree-to-tree diffs etc)

use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::{BitEntryIterator, TreeIter, TreeIterator};
use crate::obj::{Tree, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;
use std::collections::HashSet;

// TODO bad name
pub trait Apply {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()>;
    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()>;
    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()>;
}

pub trait Differ<'r>: Apply {
    fn index_mut(&mut self) -> &mut BitIndex<'r>;
}

pub trait DiffBuilder<'r>: Differ<'r> {
    /// the type of the resulting diff (returned by `Self::build_diff`)
    type Diff;
    fn build_diff(self) -> BitResult<Self::Diff>;
}

pub trait TreeDiffer<'r> {
    /// unmatched new entry
    fn on_created(&mut self, new: TreeEntry) -> BitResult<()>;
    /// called when two entries are matched (could possibly be the same entry)
    fn on_match(&mut self, old: TreeEntry, new: TreeEntry) -> BitResult<()>;
    /// unmatched old entry
    fn on_deleted(&mut self, old: TreeEntry) -> BitResult<()>;
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
                    match old.cmp(new) {
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

#[derive(Debug, Default)]
pub struct TreeDiff {
    pub new: Vec<TreeEntry>,
    pub modified: Vec<(TreeEntry, TreeEntry)>,
    pub deleted: Vec<TreeEntry>,
}

pub struct TreeDifferImpl<'r> {
    // need an inner type otherwise we can't use `GenericDiffer::new` as it takes self but also needs `a` and `b`
    repo: BitRepo<'r>,
    a: TreeIter<'r>,
    b: TreeIter<'r>,
    diff: TreeDiff,
}

impl<'r> TreeDifferImpl<'r> {
    pub fn new(repo: BitRepo<'r>, a: &Tree, b: &Tree) -> Self {
        let a = repo.tree_iter(a);
        let b = repo.tree_iter(b);
        Self { repo, a, b, diff: Default::default() }
    }
}

impl<'r> TreeDifferImpl<'r> {
    fn build_diff(mut self) -> BitResult<TreeDiff> {
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
        if old.oid == new.oid && old.mode.is_dir() && new.mode.is_dir() {
            // if hashes match and both are directories we can step over them
            self.a.over()?;
            self.b.over()?;
        } else {
            if old.oid != new.oid {
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

impl<'r> BitRepo<'r> {
    pub fn diff_tree_index(self, tree: &Tree, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_tree(tree, pathspec))
    }

    pub fn diff_index_worktree(self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_worktree(pathspec))
    }

    pub fn diff_head_index(self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.with_index_mut(|index| index.diff_head(pathspec))
    }

    pub fn diff_tree_to_tree(self, a: &Tree, b: &Tree) -> BitResult<TreeDiff> {
        TreeDifferImpl::new(self, a, b).build_diff()
    }
}

impl<'r> BitIndex<'r> {
    pub fn diff_worktree(&mut self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        IndexWorktreeDiffer::new(self, pathspec).build_diff()
    }

    pub fn diff_tree(&mut self, tree: &Tree, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        TreeIndexDiffer::new(self, tree, pathspec).build_diff()
    }

    pub fn diff_head(&mut self, pathspec: Pathspec) -> BitResult<WorkspaceDiff> {
        self.diff_tree(&self.repo.head_tree()?, pathspec)
    }
}

pub(crate) struct TreeIndexDiffer<'a, 'r> {
    repo: BitRepo<'r>,
    index: &'a mut BitIndex<'r>,
    tree: &'a Tree,
    pathspec: Pathspec,
    new: Vec<BitIndexEntry>,
    staged: Vec<(BitIndexEntry, BitIndexEntry)>,
    deleted: Vec<BitIndexEntry>,
}

impl<'a, 'r> TreeIndexDiffer<'a, 'r> {
    pub fn new(index: &'a mut BitIndex<'r>, tree: &'a Tree, pathspec: Pathspec) -> Self {
        let repo = index.repo;
        Self {
            index,
            repo,
            tree,
            pathspec,
            new: Default::default(),
            staged: Default::default(),
            deleted: Default::default(),
        }
    }
}

impl<'a, 'r> DiffBuilder<'r> for TreeIndexDiffer<'a, 'r> {
    type Diff = WorkspaceDiff;

    fn build_diff(mut self) -> BitResult<WorkspaceDiff> {
        let repo = self.repo;
        let tree_iter = self.pathspec.match_tree(repo, self.tree)?;
        let index_iter = self.pathspec.match_index(self.index)?;
        GenericDiffer::run(&mut self, tree_iter, index_iter)?;
        Ok(WorkspaceDiff { deleted: self.deleted, modified: self.staged, new: self.new })
    }
}

impl<'a, 'r> Apply for TreeIndexDiffer<'a, 'r> {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
        Ok(self.new.push(*new))
    }

    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.path, new.path);
        Ok(self.staged.push((*old, *new)))
    }

    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(*old))
    }
}

impl<'a, 'r> Differ<'r> for TreeIndexDiffer<'a, 'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
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
    fn apply<A: Apply>(&self, applier: &mut A) -> BitResult<()>;
}

impl Diff for WorkspaceDiff {
    fn apply<A: Apply>(&self, applier: &mut A) -> BitResult<()> {
        for deleted in self.deleted.iter() {
            applier.on_deleted(deleted)?;
        }
        for (old, new) in self.modified.iter() {
            applier.on_modified(old, new)?;
        }
        for new in self.new.iter() {
            applier.on_created(new)?;
        }
        Ok(())
    }
}

pub(crate) struct IndexWorktreeDiffer<'a, 'r> {
    repo: BitRepo<'r>,
    index: &'a mut BitIndex<'r>,
    pathspec: Pathspec,
    untracked: Vec<BitIndexEntry>,
    modified: Vec<(BitIndexEntry, BitIndexEntry)>,
    deleted: Vec<BitIndexEntry>,
    // directories that only contain untracked files
    _untracked_dirs: HashSet<BitPath>,
}

impl<'a, 'r> IndexWorktreeDiffer<'a, 'r> {
    pub fn new(index: &'a mut BitIndex<'r>, pathspec: Pathspec) -> Self {
        let repo = index.repo;
        Self {
            index,
            repo,
            pathspec,
            untracked: Default::default(),
            modified: Default::default(),
            deleted: Default::default(),
            _untracked_dirs: Default::default(),
        }
    }
}

impl<'a, 'r> DiffBuilder<'r> for IndexWorktreeDiffer<'a, 'r> {
    type Diff = WorkspaceDiff;

    fn build_diff(mut self) -> BitResult<WorkspaceDiff> {
        let repo = self.repo;
        let index_iter = self.pathspec.match_index(self.index)?;
        let worktree_iter = self.pathspec.match_worktree(repo)?;
        GenericDiffer::run(&mut self, index_iter, worktree_iter)?;
        Ok(WorkspaceDiff { new: self.untracked, modified: self.modified, deleted: self.deleted })
    }
}

impl<'a, 'r> Apply for IndexWorktreeDiffer<'a, 'r> {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
        self.untracked.push(*new);
        Ok(())
    }

    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.path, new.path);
        Ok(self.modified.push((*old, *new)))
    }

    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(*old))
    }
}

impl<'a, 'r> Differ<'r> for IndexWorktreeDiffer<'a, 'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
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
                // a head_entry should never reach this section as it should always have a known hash
                // (from the TreeEntry). To assert this we just check the filepath to be empty
                // (as this is the default value given when a tree entry is converted to an index entry)
                debug_assert!(!old.path.is_empty());

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
