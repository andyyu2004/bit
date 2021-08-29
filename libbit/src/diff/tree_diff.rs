use crate::obj::FileMode;

use super::*;

pub struct TreeDiffDriver<D, I, J> {
    differ: D,
    old_iter: I,
    new_iter: J,
}

pub trait TreeDiffBuilder: TreeDiffer + Sized {
    type Output;
    fn build_diff(
        mut self,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
    ) -> BitResult<Self::Output> {
        self.run_diff(old_iter, new_iter)?;
        Ok(self.get_output())
    }

    fn get_output(self) -> Self::Output;
}

impl<'a, D: TreeDiffer + ?Sized> TreeDiffer for &'a mut D {
    fn created_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        (**self).created_tree(entries)
    }

    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()> {
        (**self).created_blob(new)
    }

    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        (**self).deleted_tree(entries)
    }

    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()> {
        (**self).deleted_blob(old)
    }

    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        (**self).modified_blob(old, new)
    }
}

pub trait TreeDiffer {
    fn run_diff(
        &mut self,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
    ) -> BitResult<()> {
        TreeDiffDriver::new(self, old_iter, new_iter).run_diff()
    }

    fn created_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()>;
    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
}

impl<D, I, J> TreeDiffDriver<D, I, J> {
    pub fn new(differ: D, old_iter: I, new_iter: J) -> Self {
        Self { differ, old_iter, new_iter }
    }
}

impl<D, I, J> TreeDiffDriver<D, I, J>
where
    D: TreeDiffer,
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    fn run_diff(mut self) -> BitResult<()> {
        trace!("TreeDifferGeneric::build_diff");
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(new)) => self.on_created(new)?,
                (Some(old), None) => self.on_deleted(old)?,
                (Some(old), Some(new)) => match old.entry_cmp(&new) {
                    Ordering::Less => self.on_deleted(old)?,
                    Ordering::Equal => self.on_matched(old, new)?,
                    Ordering::Greater => self.on_created(new)?,
                },
            };
        }
        Ok(())
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_created(new: {})", new.path());
        if new.is_tree() {
            self.differ.created_tree(TreeEntriesConsumer::new(&mut self.new_iter))
        } else {
            self.new_iter.next()?;
            self.differ.created_blob(new)
        }
    }

    fn on_matched(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_match(path: {})", new.path());
        // One of the oid's may be unknown due to being either a pseudotree (index_tree_iter)
        // or just a tree (worktree_tree_iter)
        // There is also the case where there are two root entries both with unknown oids which is ok
        debug_assert!(
            old.oid().is_known()
                || new.oid().is_known()
                || old.path == BitPath::EMPTY && new.path == BitPath::EMPTY
        );
        match (old.mode(), new.mode()) {
            (FileMode::TREE, FileMode::TREE) if old.oid().is_known() && old == new => {
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
                debug_assert!(
                    old.oid().is_known() && new.oid().is_known(),
                    "all non-tree entries should have known oids"
                );
                // non-matching files
                trace!(
                    "TreeDifferGeneric::on_matched modified `{}`, {} -> {}",
                    old.path,
                    old.oid,
                    new.oid
                );
                self.differ.modified_blob(old, new)?;
                self.old_iter.next()?;
                self.new_iter.next()?;
            }
        };

        Ok(())
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        trace!("TreeDifferGeneric::on_deleted(old: {})", old.path());
        if old.is_tree() {
            self.differ.deleted_tree(TreeEntriesConsumer::new(&mut self.old_iter))
        } else {
            self.old_iter.next()?;
            self.differ.deleted_blob(old)
        }
    }
}

/// the consumer can either choose to just step over the tree or step over the tree and collect all its subentries
#[must_use = "The tree iterator will not advance until one of the available methods have been called"]
pub struct TreeEntriesConsumer<'a> {
    iter: &'a mut dyn BitTreeIterator,
}

impl<'a> TreeEntriesConsumer<'a> {
    fn new(iter: &'a mut dyn BitTreeIterator) -> Self {
        Self { iter }
    }

    /// step over the tree and return the entry of the tree itself
    pub fn step_over(self) -> BitResult<BitIndexEntry> {
        Ok(self.iter.over()?.expect("there is definitely something as we peeked this entry"))
    }

    /// appends all the non-tree subentries of the tree to `container`
    pub fn collect_over_all(self, container: &mut Vec<BitIndexEntry>) -> BitResult<BitIndexEntry> {
        self.iter.collect_over_tree_all(container)
    }

    pub fn collect_over_files(
        self,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<BitIndexEntry> {
        self.iter.collect_over_tree_files(container)
    }
}

#[derive(Default)]
pub struct TreeStatusDiffer {
    pub status: WorkspaceStatus,
}

impl TreeDiffBuilder for TreeStatusDiffer {
    type Output = WorkspaceStatus;

    fn get_output(self) -> Self::Output {
        debug_assert!(self.status.new.is_sorted_by(BitEntry::entry_partial_cmp));
        debug_assert!(self.status.deleted.is_sorted_by(BitEntry::entry_partial_cmp));
        self.status
    }
}

impl TreeDiffer for TreeStatusDiffer {
    fn created_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        entries.collect_over_files(&mut self.status.new)?;
        Ok(())
    }

    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.new.push(new))
    }

    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        entries.collect_over_files(&mut self.status.deleted)?;
        Ok(())
    }

    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.deleted.push(old))
    }

    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.modified.push((old, new)))
    }
}
