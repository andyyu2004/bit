use crate::obj::FileMode;

use super::*;

pub struct TreeDiffDriver<D, I, J> {
    differ: D,
    old_iter: I,
    new_iter: J,
}

impl<D, I, J> TreeDiffDriver<D, I, J> {
    pub fn new(differ: D, old_iter: I, new_iter: J) -> Self {
        Self { differ, old_iter, new_iter }
    }
}

pub trait TreeDiffer: Sized {
    fn run_diff(
        mut self,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
    ) -> BitResult<Self> {
        TreeDiffDriver::new(&mut self, old_iter, new_iter).build_diff()?;
        Ok(self)
    }

    fn created_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()>;
    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
}

impl<'a, D: TreeDiffer> TreeDiffer for &'a mut D {
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

impl<D, I, J> TreeDiffDriver<D, I, J>
where
    D: TreeDiffer,
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    fn build_diff(mut self) -> BitResult<()> {
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

    pub fn step_over(self) -> BitResult<Option<BitIndexEntry>> {
        self.iter.over()
    }

    pub fn collect_over(self, container: &mut Vec<BitIndexEntry>) -> BitResult<()> {
        self.iter.collect_over_tree(container)
    }
}

#[derive(Default)]
pub struct TreeStatusDiffer {
    pub status: WorkspaceStatus,
}

impl TreeDiffer for TreeStatusDiffer {
    fn created_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        entries.collect_over(&mut self.status.new)
    }

    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.new.push(new))
    }

    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        entries.collect_over(&mut self.status.deleted)
    }

    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.deleted.push(old))
    }

    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.status.modified.push((old, new)))
    }
}
