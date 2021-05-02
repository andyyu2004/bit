use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::BitIterator;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::{FallibleIterator, Fuse, Peekable};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

pub trait Differ<'r> {
    /// the type of the resulting diff (returned by `Self::run_diff`)
    type Diff;
    fn index_mut(&mut self) -> &mut BitIndex<'r>;
    fn run_diff(self) -> BitResult<Self::Diff>;
    fn on_created(&mut self, _new: BitIndexEntry) -> BitResult<()>;
    fn on_modified(&mut self, _old: BitIndexEntry, _new: BitIndexEntry) -> BitResult<()>;
    fn on_deleted(&mut self, _old: BitIndexEntry) -> BitResult<()>;
}

pub struct GenericDiffer<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitIterator,
    J: BitIterator,
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

impl<'r> BitIndex<'r> {
    //? maybe the parameters to this function need to be less general
    //? and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
    /// determine's whether `new` is *definitely* different from `old`
    // (preferably without comparing hashes)
    pub fn has_changes(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        trace!("BitIndex::has_changes({} -> {})?", old.filepath, new.filepath);
        // should only be comparing the same file
        assert_eq!(old.filepath, new.filepath);
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
                assert!(!old.filepath.is_empty());

                // file may have changed, but we are not certain, so check the hash
                let mut new_hash = new.hash;
                if new_hash.is_zero() {
                    new_hash = self.repo.hash_blob(new.filepath)?;
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
            debug!("{} unchanged: hashes match {} {}", old.filepath, old.hash, new.hash);
            return Ok(Changed::No);
        }

        if old.mtime == new.mtime {
            if self.is_racy_entry(old) {
                debug!("racy entry {}", new.filepath);
                return Ok(Changed::Maybe);
            }
            debug!("{} unchanged: non-racy mtime match {} {}", old.filepath, old.mtime, new.mtime);
            return Ok(Changed::No);
        }

        // these checks confirm if the entry definitely have changed
        // could probably add in a few of the other fields but not that important?

        if old.filesize != new.filesize {
            debug!("{} changed: filesize {} -> {}", old.filepath, old.filesize, new.filesize);
            return Ok(Changed::Yes);
        }

        if old.inode != new.inode {
            debug!("{} changed: inode {} -> {}", old.filepath, old.inode, new.inode);
            return Ok(Changed::Yes);
        }

        if tls::with_config(|config| config.filemode())? && old.mode != new.mode {
            debug!("{} changed: filemode {} -> {}", old.filepath, old.mode, new.mode);
            return Ok(Changed::Yes);
        }

        debug!("{} uncertain if changed", old.filepath);

        Ok(Changed::Maybe)
    }
}

impl<'d, 'r, D, I, J> GenericDiffer<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitIterator,
    J: BitIterator,
{
    fn new(differ: &'d mut D, old_iter: I, new_iter: J) -> Self {
        Self {
            old_iter: old_iter.fuse().peekable(),
            new_iter: new_iter.fuse().peekable(),
            differ,
            pd: std::marker::PhantomData,
        }
    }

    pub fn run(differ: &'d mut D, old_iter: I, new_iter: J) -> BitResult<()> {
        Self::new(differ, old_iter, new_iter).diff_generic()
    }

    fn handle_delete(&mut self, old: BitIndexEntry) -> BitResult<()> {
        self.old_iter.next()?;
        self.differ.on_deleted(old)
    }

    fn handle_create(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.new_iter.next()?;
        self.differ.on_created(new)
    }

    fn handle_potential_update(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        self.old_iter.next()?;
        self.new_iter.next()?;
        // if we are here then we know that the path and stage of the entries match
        // however, that does not mean that the file has not changed
        if self.differ.index_mut().has_changes(&old, &new)? {
            self.differ.on_modified(old, new)?;
        }
        Ok(())
    }

    pub fn diff_generic(&mut self) -> BitResult<()> {
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(&new)) => self.handle_create(new)?,
                (Some(&old), None) => self.handle_delete(old)?,
                (Some(&old), Some(&new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match old.cmp(&new) {
                        Ordering::Less => self.handle_delete(old)?,
                        Ordering::Equal => self.handle_potential_update(old, new)?,
                        Ordering::Greater => self.handle_create(new)?,
                    }
                }
            };
        }
        Ok(())
    }
}

impl BitRepo {
    pub fn diff_index_worktree(&self) -> BitResult<IndexWorktreeDiff> {
        self.with_index_mut(|index| IndexWorktreeDiffer::new(index).run_diff())
    }

    pub fn diff_head_index(&self) -> BitResult<HeadIndexDiff> {
        self.with_index_mut(|index| HeadIndexDiffer::new(index).run_diff())
    }
}

pub(crate) struct HeadIndexDiffer<'a, 'r> {
    repo: &'r BitRepo,
    index: &'a mut BitIndex<'r>,
    new: Vec<BitPath>,
    staged: Vec<BitPath>,
    deleted: Vec<BitPath>,
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

impl<'a, 'r> Differ<'r> for HeadIndexDiffer<'a, 'r> {
    type Diff = HeadIndexDiff;

    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
    }

    fn run_diff(mut self) -> BitResult<HeadIndexDiff> {
        let repo = self.repo;
        let index_iter = self.index.iter();
        GenericDiffer::run(&mut self, repo.head_iter()?, index_iter)?;
        Ok(HeadIndexDiff { deleted: self.deleted, modified: self.staged, new: self.new })
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.new.push(new.filepath))
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(old.filepath))
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.filepath, new.filepath);
        Ok(self.staged.push(new.filepath))
    }
}

#[derive(Debug)]
pub struct HeadIndexDiff {
    pub new: Vec<BitPath>,
    pub modified: Vec<BitPath>,
    pub deleted: Vec<BitPath>,
}

impl HeadIndexDiff {
    pub fn is_empty(&self) -> bool {
        self.new.is_empty() && self.deleted.is_empty() && self.modified.is_empty()
    }
}

#[derive(Debug)]
pub struct IndexWorktreeDiff {
    pub untracked: Vec<BitPath>,
    pub modified: Vec<BitPath>,
    pub deleted: Vec<BitPath>,
}

impl IndexWorktreeDiff {
    pub fn is_empty(&self) -> bool {
        self.untracked.is_empty() && self.deleted.is_empty() && self.modified.is_empty()
    }
}

pub(crate) struct IndexWorktreeDiffer<'a, 'r> {
    repo: &'r BitRepo,
    index: &'a mut BitIndex<'r>,
    untracked: Vec<BitPath>,
    modified: Vec<BitPath>,
    deleted: Vec<BitPath>,
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

impl<'a, 'r> Differ<'r> for IndexWorktreeDiffer<'a, 'r> {
    type Diff = IndexWorktreeDiff;

    fn run_diff(mut self) -> BitResult<IndexWorktreeDiff> {
        let repo = self.repo;
        let index_iter = self.index.iter();
        GenericDiffer::run(&mut self, index_iter, repo.worktree_iter()?)?;
        Ok(IndexWorktreeDiff {
            untracked: self.untracked,
            modified: self.modified,
            deleted: self.deleted,
        })
    }

    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.untracked.push(new.filepath);
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.filepath, new.filepath);
        Ok(self.modified.push(new.filepath))
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.deleted.push(old.filepath))
    }
}

#[cfg(test)]
mod tests;
