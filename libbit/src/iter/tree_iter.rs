use super::*;
use crate::diff::TreeEntriesConsumer;
use crate::index::BitTreeCache;
use crate::obj::MutableTree;
use std::iter::FromIterator;

pub type SkipTrees<I> = fallible_iterator::Filter<I, fn(&BitIndexEntry) -> BitResult<bool>>;

/// Tree iterators allow stepping over entire trees (skipping all entries recursively)
// The methods on this trait yield index_entries rather than tree_entries as index_entries are strictly more general
// In particular, the index tree iterator implements this trait and we don't want to lose the information in the index entry.
// On the otherhand, its ok to just fill the extra fields with sentinels.
// Considered using an enum holding either a tree entry or an index entry but it didn't seem worth it
pub trait BitTreeIterator: BitIterator<BitIndexEntry> {
    /// unstable semantics
    /// if the next entry is a tree then yield the tree entry but skip over its contents
    /// otherwise does the same as next
    /// `next` should always yield the tree entry itself
    /// if the `peeked` entry is a directory and over is called, then that entry should be yielded
    /// and its contents skipped over
    /// full paths should be returned (relative to repo root), not just relative to the parent
    fn over(&mut self) -> BitResult<Option<Self::Item>>;

    // seems difficult to provide a peek method just via an adaptor
    // unclear how to implement peek in terms of `over` and `next`
    // in particular for the case of `TreeIter`,
    // if `peek` uses `next`, then all the subdirectories would already
    // be added to the stack and its awkward to implement `over` after `peek`
    // similar problems arise with implementing `peek` using `over`
    // probably better to just let the implementor deal with it
    // especially as the implementation is probably trivial
    fn peek(&mut self) -> BitResult<Option<Self::Item>>;

    fn skip_trees(self) -> SkipTrees<Self>
    where
        Self: Sized,
    {
        self.filter(|entry| Ok(!entry.is_tree()))
    }

    fn as_consumer(&mut self) -> TreeEntriesConsumer<'_>
    where
        Self: Sized,
    {
        TreeEntriesConsumer::new(self)
    }

    // TODO these names all suck
    fn collect_over_tree_blobs(
        &mut self,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<Self::Item> {
        self.collect_over_tree_filtered(|entry| entry.is_blob(), container)
    }

    fn collect_over_tree_all(
        &mut self,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<Self::Item> {
        self.collect_over_tree_filtered(|_| true, container)
    }

    /// Same as `self.over` but instead appends all the entries that satisfy the predicate into a container.
    /// This takes a container to append to instead of returning a vec to avoid a separate allocation.
    /// Returns the tree_entry.
    fn collect_over_tree_filtered(
        &mut self,
        predicate: fn(&BitIndexEntry) -> bool,
        container: &mut Vec<BitIndexEntry>,
    ) -> BitResult<Self::Item> {
        let tree_entry = self.next()?.expect("currently expected to not be called when at end");
        let tree_entry_path = tree_entry.path();
        debug_assert_eq!(tree_entry.mode(), FileMode::TREE);
        while self.peek()?.map(|next| next.path().starts_with(tree_entry_path)).unwrap_or(false) {
            let entry = self.next()?.unwrap();
            if predicate(&entry) {
                container.push(entry);
            }
        }
        Ok(tree_entry)
    }

    /// creates a tree object from a tree_iterator
    /// the tree_iterator must be "fresh" and completely unconsumed
    fn build_tree(&mut self, repo: BitRepo<'_>, tree_cache: Option<&BitTreeCache>) -> BitResult<Oid>
    where
        Self: Sized,
    {
        // skip root entry
        let root = self.next()?.unwrap();
        let oid = build_tree_internal(repo, self, tree_cache, BitPath::EMPTY)?;
        debug_assert!(root.oid.is_unknown() || root.oid == oid);
        Ok(oid)
    }
}

impl<'a> dyn BitTreeIterator + 'a {
    #[doc(hidden)]
    /// Create an iterator that yields all files within the current subtree
    // Only for use from tree_entries consumer
    pub(crate) fn collect_over_tree_iter(&'a mut self) -> impl BitIterator<BitIndexEntry> + 'a {
        let entry =
            self.peek().expect("should have been successfully peeked before calling this").unwrap();
        assert_eq!(entry.mode(), FileMode::TREE);
        CollectTree { iter: self, tree_entry: TreeEntry::from(entry) }
    }

    pub(crate) fn collect_over_tree_files_iter(
        &'a mut self,
    ) -> impl BitIterator<BitIndexEntry> + 'a {
        self.collect_over_tree_iter().filter(|entry: &BitIndexEntry| Ok(entry.is_blob()))
    }
}

impl<'a, I: BitTreeIterator> BitTreeIterator for &'a mut I {
    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        (**self).over()
    }

    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        (**self).peek()
    }
}

fn build_tree_internal(
    repo: BitRepo<'_>,
    iter: &mut impl BitTreeIterator,
    tree_cache: Option<&BitTreeCache>,
    base_path: BitPath,
) -> BitResult<Oid> {
    let mut entries = vec![];
    loop {
        let entry = match iter.peek()? {
            Some(entry) if entry.path.starts_with(base_path) => match entry.mode {
                FileMode::REG | FileMode::EXEC | FileMode::LINK => {
                    iter.next()?;
                    TreeEntry { oid: entry.oid, mode: entry.mode, path: entry.path.file_name() }
                }
                FileMode::TREE => {
                    let child = tree_cache.and_then(|cache| cache.find_valid_child(entry.path));
                    let tree_exists = child
                        .map(|cache| cache.is_valid() && cache.tree_oid == entry.oid)
                        .unwrap_or(false);
                    let oid = if tree_exists {
                        // the tree already exists so we just step over the tree and takes it's oid
                        iter.over()?.unwrap().oid
                    } else {
                        iter.next()?;
                        build_tree_internal(repo, iter, tree_cache, entry.path)?
                    };
                    TreeEntry { oid, mode: FileMode::TREE, path: entry.path.file_name() }
                }
                FileMode::GITLINK => todo!(),
            },
            _ => break,
        };
        entries.push(entry);
    }

    debug_assert!(entries.is_sorted());
    let tree = MutableTree::from_iter(entries);
    repo.write_obj(&tree)
}

#[must_use]
pub struct CollectTree<'a> {
    tree_entry: TreeEntry,
    iter: &'a mut dyn BitTreeIterator,
}

impl FallibleIterator for CollectTree<'_> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let tree_entry_path = self.tree_entry.path;
        if self.iter.peek()?.map(|next| next.path().starts_with(tree_entry_path)).unwrap_or(false) {
            let entry = self.iter.next()?.unwrap();
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }
}

impl<I, F> BitTreeIterator for fallible_iterator::Filter<I, F>
where
    I: BitTreeIterator,
    F: FnMut(&I::Item) -> Result<bool, I::Error>,
{
    fn over(&mut self) -> BitResult<Option<I::Item>> {
        let mut next = self.it.over()?;
        while let Some(x) = next {
            if (self.f)(&x)? {
                break;
            }
            next = self.it.over()?;
        }
        Ok(next)
    }

    fn peek(&mut self) -> BitResult<Option<I::Item>> {
        loop {
            match self.it.peek()? {
                Some(x) if (self.f)(&x)? => break,
                None => break,
                _ => self.it.next()?,
            };
        }
        self.it.peek()
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn head_tree_iter(self) -> BitResult<TreeIter<'rcx>> {
        let oid = self.head_tree()?;
        Ok(self.tree_iter(oid))
    }

    /// Return's tree iterator for a tree (or treeish object) with oid = `oid`
    /// It is valid to pass `Oid::UNKNOWN` which will represent an empty iterator which only yields the root
    // We can't use an `impl Treeish` here as the above case will not work
    pub fn tree_iter(self, oid: Oid) -> TreeIter<'rcx> {
        TreeIter::new(self, oid)
    }
}

#[derive(Debug)]
pub struct TreeIter<'rcx> {
    repo: BitRepo<'rcx>,
    /// The current stack of entries.
    /// Each entry should be modified to have the full path (relative to the repo root)
    /// not just relative to its parent.
    entry_stack: Vec<TreeEntry>,
    /// The number of entries in the stack before the most recent directory was pushed.f
    /// This is used for stepping over
    previous_len: usize,
}

impl<'rcx> TreeIter<'rcx> {
    pub fn new(repo: BitRepo<'rcx>, oid: Oid) -> Self {
        debug_assert!(oid.is_unknown() || repo.read_obj(oid).unwrap().is_treeish());
        let entry_stack = vec![TreeEntry { oid, path: BitPath::EMPTY, mode: FileMode::TREE }];
        Self { repo, previous_len: 0, entry_stack }
    }
}

impl<'rcx> BitTreeIterator for TreeIter<'rcx> {
    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        match self.next()? {
            Some(entry) => {
                if entry.mode() == FileMode::TREE {
                    // we implement stepping over by keeping track of the stack height (in `Self::next`)
                    // if `next` yields a tree entry we can skip over all the new entries by just throwing away
                    // all the new entries on the stack
                    // the `previous_len` is always up to date as we just called `next` which must have run through the
                    // `FileMode::Tree` path
                    self.entry_stack.truncate(self.previous_len);
                }
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        Ok(self.entry_stack.last().map(|&entry| entry.into()))
    }
}

impl<'rcx> FallibleIterator for TreeIter<'rcx> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.entry_stack.pop() {
                Some(entry) => match entry.mode {
                    FileMode::TREE => {
                        // special case for when the initial `oid` is unknown then we just want an iterator that yields only the root
                        if entry.oid.is_unknown() {
                            debug_assert!(self.entry_stack.is_empty());
                            debug_assert_eq!(entry.path, BitPath::EMPTY);
                            return Ok(Some(entry.into()));
                        }

                        let tree = entry.oid.treeish(self.repo)?;
                        trace!("TreeIter::next: read directory `{:?}` `{}`", entry.path, entry.oid);

                        self.previous_len = self.entry_stack.len();
                        self.entry_stack.extend(
                            tree.entries
                                .iter()
                                .copied()
                                .rev()
                                // TODO we have to filter out here for now otherwise peek may blow up
                                .filter(|entry| entry.mode != FileMode::GITLINK)
                                .map(|mut next_entry| {
                                    // convert the `relative_to_parent` path to a `relative_to_repo_root` path
                                    next_entry.path = entry.path.join(next_entry.path);
                                    next_entry
                                }),
                        );

                        return Ok(Some(entry.into()));
                    }
                    FileMode::REG | FileMode::LINK | FileMode::EXEC => {
                        trace!("TreeIter::next: entry: {:?}", entry);
                        debug_assert!(entry.oid.is_known());
                        return Ok(Some(entry.into()));
                    }
                    // ignore submodules for now
                    FileMode::GITLINK => continue,
                },
                None => return Ok(None),
            }
        }
    }
}
