use super::*;
use crate::error::BitGenericError;
use crate::iter::{BitIterator, IterKind};
use crate::obj::{FileMode, TreeEntry};
use fallible_iterator::FallibleLendingIterator;
use std::cell::RefCell;
use std::fmt::{self, Debug, Formatter};

#[derive(Debug, Default)]
pub struct DiffOpts {
    pub flags: DiffOptFlags,
}

impl DiffOpts {
    pub const INCLUDE_UNMODIFIED: Self = Self { flags: DiffOptFlags::INCLUDE_UNMODIFIED };

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

#[derive(Debug)]
pub enum TreeDiffEntry<'a> {
    DeletedBlob(BitIndexEntry),
    CreatedBlob(BitIndexEntry),
    ModifiedBlob(BitIndexEntry, BitIndexEntry),
    // This is a bit of a edge case, both iterators will always step into the trees.
    // This is just letting us know the tree entry exists.
    // This is necessary for the worktree in checkout to matchup with the treediff correctly
    MaybeModifiedTree(BitIndexEntry),
    UnmodifiedBlob(BitIndexEntry),
    UnmodifiedTree(TreeEntriesConsumer<'a>),
    DeletedTree(TreeEntriesConsumer<'a>),
    CreatedTree(TreeEntriesConsumer<'a>),
    BlobToTree(BitIndexEntry, TreeEntriesConsumer<'a>),
    TreeToBlob(TreeEntriesConsumer<'a>, BitIndexEntry),
}

impl BitEntry for TreeDiffEntry<'_> {
    fn oid(&self) -> Oid {
        self.index_entry().oid
    }

    fn path(&self) -> BitPath {
        self.index_entry().path
    }

    fn mode(&self) -> FileMode {
        self.index_entry().mode
    }
}

impl<'a> TreeDiffEntry<'a> {
    pub(crate) fn index_entry(&self) -> BitIndexEntry {
        match self {
            TreeDiffEntry::DeletedBlob(old) => *old,
            TreeDiffEntry::CreatedBlob(new) => *new,
            TreeDiffEntry::ModifiedBlob(_, new) => *new,
            TreeDiffEntry::MaybeModifiedTree(entry) => *entry,
            TreeDiffEntry::DeletedTree(old) => old.peek(),
            TreeDiffEntry::CreatedTree(new) => new.peek(),
            TreeDiffEntry::UnmodifiedBlob(entry) => *entry,
            TreeDiffEntry::UnmodifiedTree(tree) => tree.peek(),
            TreeDiffEntry::BlobToTree(_, new) => new.peek(),
            TreeDiffEntry::TreeToBlob(_, new) => *new,
        }
    }
}

pub struct TreeDiffIter<'rcx, I, J> {
    repo: BitRepo<'rcx>,
    old_iter: I,
    new_iter: J,
    opts: DiffOpts,
}

impl<'rcx, I: BitTreeIterator, J> TreeDiffIter<'rcx, I, J> {
    pub fn new(repo: BitRepo<'rcx>, old_iter: I, new_iter: J, opts: DiffOpts) -> Self {
        assert_ne!(
            old_iter.kind(),
            IterKind::Worktree,
            "old iterator is not allowed to be a worktree iterator"
        );
        Self { repo, old_iter, new_iter, opts }
    }
}

pub trait TreeDiffIterator<'a>:
    FallibleLendingIterator<Item<'a> = TreeDiffEntry<'a>, Error = BitGenericError> + 'a
{
}

impl<'a, I> TreeDiffIterator<'a> for I where
    I: FallibleLendingIterator<Item<'a> = TreeDiffEntry<'a>, Error = BitGenericError> + 'a
{
}

impl<'rcx, I, J> TreeDiffIter<'rcx, I, J>
where
    I: BitTreeIterator + 'rcx,
    J: BitTreeIterator + 'rcx,
{
    pub fn into_iter(self) -> impl TreeDiffIterator<'rcx> {
        self
    }
}

impl<'rcx, I, J> FallibleLendingIterator for TreeDiffIter<'rcx, I, J>
where
    I: BitTreeIterator,
    J: BitTreeIterator,
{
    type Error = BitGenericError;
    type Item<'a> = TreeDiffEntry<'a> where Self: 'a;

    fn next(&mut self) -> Result<Option<Self::Item<'_>>, Self::Error> {
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => return Ok(None),
                (None, Some(new)) => return self.on_created(new),
                (Some(old), None) => return self.on_deleted(old),
                (Some(old), Some(mut new)) => match old.diff_cmp(&new) {
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
                            (FileMode::GITLINK, _) | (_, FileMode::GITLINK) => todo!("submodules"),
                            (FileMode::TREE, FileMode::TREE)
                                if old.oid().is_known() && old == new =>
                            {
                                // If hashes match and both are directories we can step over them
                                // (unless we want to include unmodified)
                                if self.opts.include_unmodified() {
                                    self.old_iter.over()?;
                                    return Ok(Some(TreeDiffEntry::UnmodifiedTree(
                                        self.new_iter.as_consumer(),
                                    )));
                                } else {
                                    self.old_iter.over()?;
                                    self.new_iter.over()?;
                                }
                            }
                            (FileMode::TREE, FileMode::TREE) => {
                                // two trees with non matching oids, then step inside for both
                                self.old_iter.next()?;
                                self.new_iter.next()?;
                                return Ok(Some(TreeDiffEntry::MaybeModifiedTree(new)));
                            }
                            (FileMode::TREE, _) => {
                                debug_assert!(new.is_blob());
                                self.new_iter.next()?;
                                return Ok(Some(TreeDiffEntry::TreeToBlob(
                                    self.old_iter.as_consumer(),
                                    new,
                                )));
                            }
                            (_, FileMode::TREE) => {
                                debug_assert!(old.is_blob());
                                self.old_iter.next()?;
                                return Ok(Some(TreeDiffEntry::BlobToTree(
                                    old,
                                    self.new_iter.as_consumer(),
                                )));
                            }
                            _ => {
                                debug_assert!(old.is_blob());
                                debug_assert!(new.is_blob());
                                debug_assert!(old.oid.is_known());

                                let has_changed = if self.new_iter.kind() == IterKind::Worktree {
                                    self.repo.index_mut()?.is_worktree_entry_modified(&mut new)?
                                } else {
                                    debug_assert!(new.oid.is_known());
                                    old.oid != new.oid
                                };

                                if !has_changed {
                                    // matching files
                                    self.old_iter.next()?;
                                    self.new_iter.next()?;
                                    if self.opts.include_unmodified() {
                                        return Ok(Some(TreeDiffEntry::UnmodifiedBlob(new)));
                                    }
                                } else {
                                    // non-matching files
                                    trace!(
                                        "TreeDifferGeneric::on_matched modified `{}`, {}({}) -> {}({}",
                                        old.path,
                                        old.oid,
                                        old.mode,
                                        new.oid,
                                        new.mode
                                    );
                                    self.old_iter.next()?;
                                    self.new_iter.next()?;

                                    return Ok(Some(TreeDiffEntry::ModifiedBlob(old, new)));
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

impl<I, J> TreeDiffIter<'_, I, J>
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
        repo: BitRepo<'_>,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
        opts: DiffOpts,
    ) -> BitResult<Self::Output> {
        self.run_diff(repo, old_iter, new_iter, opts)?;
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

    fn tree_to_blob(&mut self, old: TreeEntriesConsumer<'_>, new: BitIndexEntry) -> BitResult<()> {
        (**self).tree_to_blob(old, new)
    }

    fn blob_to_tree(&mut self, old: BitIndexEntry, new: TreeEntriesConsumer<'_>) -> BitResult<()> {
        (**self).blob_to_tree(old, new)
    }
}

pub trait TreeDiffer {
    fn run_diff(
        &mut self,
        repo: BitRepo<'_>,
        old_iter: impl BitTreeIterator,
        new_iter: impl BitTreeIterator,
        opts: DiffOpts,
    ) -> BitResult<()> {
        todo!()
        // TreeDiffIter::new(repo, old_iter, new_iter, opts).into_iter().for_each(|diff_entry| {
        //     match diff_entry {
        //         TreeDiffEntry::DeletedBlob(old) => self.deleted_blob(old),
        //         TreeDiffEntry::CreatedBlob(new) => self.created_blob(new),
        //         TreeDiffEntry::ModifiedBlob(old, new) => self.modified_blob(old, new),
        //         TreeDiffEntry::DeletedTree(old_entries) => self.deleted_tree(old_entries),
        //         TreeDiffEntry::CreatedTree(new_entries) => self.created_tree(new_entries),
        //         TreeDiffEntry::BlobToTree(blob, tree) => self.blob_to_tree(blob, tree),
        //         TreeDiffEntry::TreeToBlob(tree, blob) => self.tree_to_blob(tree, blob),
        //         TreeDiffEntry::MaybeModifiedTree(..) => Ok(()),
        //         TreeDiffEntry::UnmodifiedBlob(..) | TreeDiffEntry::UnmodifiedTree(..) =>
        //             panic!("included unmodified files when calculating a diff?"),
        //     }
        // })
    }

    fn created_tree(&mut self, new_entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()>;
    fn deleted_tree(&mut self, old_entries: TreeEntriesConsumer<'_>) -> BitResult<()>;
    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()>;
    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()>;
    fn tree_to_blob(&mut self, old: TreeEntriesConsumer<'_>, new: BitIndexEntry) -> BitResult<()>;
    fn blob_to_tree(&mut self, old: BitIndexEntry, new: TreeEntriesConsumer<'_>) -> BitResult<()>;
}

/// The consumer can either choose to just step over the tree or step over the tree and collect all its subentries
/// The tree will be stepped over if still not consumed when dropped
pub struct TreeEntriesConsumer<'a> {
    // refcell is here mostly so `peek` doesn't require a mutable reference
    iter: Option<RefCell<&'a mut dyn BitTreeIterator>>,
}

impl std::fmt::Debug for TreeEntriesConsumer<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.peek())
    }
}

impl<'a> Drop for TreeEntriesConsumer<'a> {
    fn drop(&mut self) {
        if let Some(iter) = self.iter.take() {
            iter.into_inner().over().expect("is it better to panic here or just ignore the error?");
        }
    }
}

impl<'a> TreeEntriesConsumer<'a> {
    pub(crate) fn new(iter: &'a mut dyn BitTreeIterator) -> Self {
        Self { iter: Some(RefCell::new(iter)) }
    }

    /// Peek the tree entry
    pub fn peek(&self) -> BitIndexEntry {
        self.iter
            .as_ref()
            .unwrap()
            .borrow_mut()
            .peek()
            .expect("peek shouldn't fail on a second call surely")
            .unwrap()
    }

    fn into_inner(mut self) -> &'a mut dyn BitTreeIterator {
        self.iter.take().unwrap().into_inner()
    }

    /// Step over the tree and return the entry of the tree itself
    pub fn step_over(self) -> BitResult<BitIndexEntry> {
        Ok(self
            .into_inner()
            .over()?
            .expect("there is definitely something as we peeked this entry"))
    }

    /// appends all the non-tree subentries of the tree to `container`
    pub fn collect_over_all(self, container: &mut Vec<BitIndexEntry>) -> BitResult<BitIndexEntry> {
        self.into_inner().collect_over_tree_all(container)
    }

    pub fn collect_over_files(
        self,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<BitIndexEntry> {
        self.into_inner().collect_over_tree_blobs(container)
    }

    pub fn iter_files(self) -> impl BitTreeIterator + 'a {
        self.into_inner().collect_over_tree_files_iter()
    }

    /// Returns an iterator that yields the root of the subtree and all its subentries
    pub fn iter(self) -> impl BitTreeIterator + 'a {
        self.into_inner().collect_over_tree_iter()
    }

    pub fn collect(self) -> BitResult<Vec<BitIndexEntry>> {
        self.into_inner().collect_over_tree_iter().collect()
    }
}

#[derive(Default)]
pub struct TreeStatusDiffer {
    pub status: WorkspaceStatus,
}

impl TreeDiffBuilder for TreeStatusDiffer {
    type Output = WorkspaceStatus;

    fn get_output(self) -> Self::Output {
        debug_assert!(self.status.new.is_sorted_by(BitEntry::diff_partial_cmp));
        debug_assert!(self.status.deleted.is_sorted_by(BitEntry::diff_partial_cmp));
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

    fn tree_to_blob(&mut self, old: TreeEntriesConsumer<'_>, new: BitIndexEntry) -> BitResult<()> {
        old.iter_files().for_each(|deleted| {
            self.status.deleted.push(deleted);
            Ok(())
        })?;
        self.status.new.push(new);
        Ok(())
    }

    fn blob_to_tree(
        &mut self,
        old: BitIndexEntry,
        new_tree: TreeEntriesConsumer<'_>,
    ) -> BitResult<()> {
        self.status.deleted.push(old);
        new_tree.iter_files().for_each(|new| {
            self.status.new.push(new);
            Ok(())
        })
    }
}
