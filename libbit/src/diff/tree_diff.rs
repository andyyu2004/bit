use super::*;
use crate::error::BitGenericError;
use crate::obj::{FileMode, TreeEntry};
use fallible_iterator::FallibleLendingIterator;
use std::cell::RefCell;

#[derive(Debug, Default)]
pub struct DiffOpts {
    pub flags: DiffOptFlags,
}

impl DiffOpts {
    pub fn with_flags(flags: DiffOptFlags) -> Self {
        Self { flags }
    }

    pub fn include_unmodified(&self) -> bool {
        self.flags.contains(DiffOptFlags::INCLUDE_UNMODIFIED)
    }
}

bitflags! {
    #[derive(Default)]
    pub struct DiffOptFlags: u8 {
        /// Yield all unmodified blobs (but not trees)
        const INCLUDE_UNMODIFIED = 1 << 0;
    }
}

pub enum TreeDiffEntry<'a> {
    DeletedBlob(BitIndexEntry),
    CreatedBlob(BitIndexEntry),
    ModifiedBlob(BitIndexEntry, BitIndexEntry),
    UnmodifiedBlob(BitIndexEntry),
    DeletedTree(TreeEntriesConsumer<'a>),
    CreatedTree(TreeEntriesConsumer<'a>),
}

impl BitEntry for TreeDiffEntry<'_> {
    fn oid(&self) -> Oid {
        self.tree_entry().oid
    }

    fn path(&self) -> BitPath {
        self.tree_entry().path
    }

    fn mode(&self) -> FileMode {
        self.tree_entry().mode
    }
}

impl<'a> TreeDiffEntry<'a> {
    pub fn index_entry(&self) -> BitIndexEntry {
        match self {
            TreeDiffEntry::DeletedBlob(old) => *old,
            TreeDiffEntry::CreatedBlob(new) => *new,
            TreeDiffEntry::ModifiedBlob(_, new) => *new,
            TreeDiffEntry::DeletedTree(old) => old.peek(),
            TreeDiffEntry::CreatedTree(new) => new.peek(),
            TreeDiffEntry::UnmodifiedBlob(entry) => *entry,
        }
    }

    // This does less copying than `index_entry`
    pub fn tree_entry(&self) -> TreeEntry {
        match self {
            TreeDiffEntry::DeletedBlob(old) => old.into(),
            TreeDiffEntry::CreatedBlob(new) => new.into(),
            // We return the `new` entry as the representive entry.
            // This is required for correctness in checkout as it uses
            // the entry returned by this function to determine what content to checkout.
            TreeDiffEntry::ModifiedBlob(_, new) => new.into(),
            TreeDiffEntry::DeletedTree(old) => old.peek().into(),
            TreeDiffEntry::CreatedTree(new) => new.peek().into(),
            TreeDiffEntry::UnmodifiedBlob(entry) => entry.into(),
        }
    }
}

pub struct TreeDiffIter<I, J> {
    old_iter: I,
    new_iter: J,
    opts: DiffOpts,
}

impl<I, J> TreeDiffIter<I, J> {
    pub fn new(old_iter: I, new_iter: J, opts: DiffOpts) -> Self {
        Self { old_iter, new_iter, opts }
    }
}

#[rustfmt::skip]
pub trait TreeDiffIterator<'a> =
    FallibleLendingIterator<Item<'a> = TreeDiffEntry<'a>, Error = BitGenericError>;

impl<I, J> FallibleLendingIterator for TreeDiffIter<I, J>
where
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    type Error = BitGenericError;
    type Item<'a> = TreeDiffEntry<'a>;

    fn next(&mut self) -> Result<Option<Self::Item<'_>>, Self::Error> {
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => return Ok(None),
                (None, Some(new)) => return self.on_created(new),
                (Some(old), None) => return self.on_deleted(old),
                (Some(old), Some(new)) => match old.entry_cmp(&new) {
                    Ordering::Less => return self.on_deleted(old),
                    Ordering::Greater => return self.on_created(new),
                    Ordering::Equal => {
                        // surely can rewrite this somehow (preferably without using a macro)
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
                            (FileMode::TREE, FileMode::TREE)
                                if old.oid().is_known() && old == new =>
                            {
                                // If hashes match and both are directories we can step over them
                                // (unless we want to include unmodified, in which case we step inside)
                                if self.opts.include_unmodified() {
                                    self.old_iter.next()?;
                                    self.new_iter.next()?;
                                } else {
                                    self.old_iter.over()?;
                                    self.new_iter.over()?;
                                }
                            }
                            (FileMode::TREE, FileMode::TREE) => {
                                // two trees with non matching oids, then step inside for both
                                self.old_iter.next()?;
                                self.new_iter.next()?;
                            }
                            _ if old.is_file() && new.is_file() && old.oid() == new.oid() => {
                                // matching files
                                debug_assert!(old.oid().is_known() && new.oid().is_known());
                                let entry = self.old_iter.next()?.unwrap();
                                self.new_iter.next()?;
                                if self.opts.include_unmodified() {
                                    return Ok(Some(TreeDiffEntry::UnmodifiedBlob(entry)));
                                }
                            }
                            _ => {
                                debug_assert!(
                                    old.is_file() && new.is_file(),
                                    "A tree vs nontree should not call `on_match`, check the ordering function.
                                    If two entries have the same path then files should come before directories (as per `entry_cmp`).
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
                                self.old_iter.next()?;
                                self.new_iter.next()?;

                                return Ok(Some(TreeDiffEntry::ModifiedBlob(old, new)));
                            }
                        }
                    }
                },
            }
        }
    }
}

impl<I, J> TreeDiffIter<I, J>
where
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<Option<TreeDiffEntry<'_>>> {
        trace!("TreeDifferGeneric::on_created(new: {})", new.path());
        if new.is_tree() {
            Ok(Some(TreeDiffEntry::CreatedTree(self.new_iter.as_consumer())))
        } else {
            self.new_iter.next()?;
            Ok(Some(TreeDiffEntry::CreatedBlob(new)))
        }
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<Option<TreeDiffEntry<'_>>> {
        trace!("TreeDifferGeneric::on_deleted(old: {})", old.path());
        if old.is_tree() {
            Ok(Some(TreeDiffEntry::DeletedTree(self.old_iter.as_consumer())))
        } else {
            self.old_iter.next()?;
            Ok(Some(TreeDiffEntry::DeletedBlob(old)))
        }
    }
}

pub trait TreeDiffBuilder: TreeDiffer + Sized {
    type Output;
    fn build_diff(
        mut self,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
        opts: DiffOpts,
    ) -> BitResult<Self::Output> {
        self.run_diff(old_iter, new_iter, opts)?;
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
        opts: DiffOpts,
    ) -> BitResult<()> {
        TreeDiffIter { old_iter, new_iter, opts }.for_each(|diff_entry| match diff_entry {
            TreeDiffEntry::DeletedBlob(old) => self.deleted_blob(old),
            TreeDiffEntry::CreatedBlob(new) => self.created_blob(new),
            TreeDiffEntry::ModifiedBlob(old, new) => self.modified_blob(old, new),
            TreeDiffEntry::DeletedTree(old_entries) => self.deleted_tree(old_entries),
            TreeDiffEntry::CreatedTree(new_entries) => self.created_tree(new_entries),
            TreeDiffEntry::UnmodifiedBlob(..) =>
                panic!("including unmodified files when calculating a diff"),
        })
    }

    fn created_tree(&mut self, new_entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn deleted_tree(&mut self, old_entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()>;
    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
}

/// The consumer can either choose to just step over the tree or step over the tree and collect all its subentries
#[must_use = "The tree iterator will not advance until one of the available methods have been called"]
pub struct TreeEntriesConsumer<'a> {
    iter: RefCell<&'a mut dyn BitTreeIterator>,
}

impl<'a> TreeEntriesConsumer<'a> {
    pub(crate) fn new(iter: &'a mut dyn BitTreeIterator) -> Self {
        Self { iter: RefCell::new(iter) }
    }

    pub fn peek(&self) -> BitIndexEntry {
        self.iter.borrow_mut().peek().expect("peek shouldn't fail on a second call surely").unwrap()
    }

    /// step over the tree and return the entry of the tree itself
    pub fn step_over(self) -> BitResult<BitIndexEntry> {
        Ok(self
            .iter
            .into_inner()
            .over()?
            .expect("there is definitely something as we peeked this entry"))
    }

    /// appends all the non-tree subentries of the tree to `container`
    pub fn collect_over_all(self, container: &mut Vec<BitIndexEntry>) -> BitResult<BitIndexEntry> {
        self.iter.into_inner().collect_over_tree_all(container)
    }

    pub fn collect_over_files(
        self,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<BitIndexEntry> {
        self.iter.into_inner().collect_over_tree_files(container)
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
