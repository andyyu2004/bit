use crate::core::BitOrd;

use super::*;

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait BitTreeIterator: BitIterator<TreeIteratorEntry> {
    /// unstable semantics
    /// if the next entry is a tree then yield the tree entry but skip over its contents
    /// otherwise does the same as next
    /// `next` should always yield the tree entry itself
    /// if the `peeked` entry is a directory and over is called, then that entry should be yielded
    /// and its contents skipped over
    fn over(&mut self) -> BitResult<Option<Self::Item>>;

    /// same as `self.over` but instead appends all the non-tree entries into a container
    fn collect_over(
        &mut self,
        container: &mut Vec<BitIndexEntry>,
        tree_entry: TreeEntry,
    ) -> BitResult<()> {
        debug_assert_eq!(tree_entry.mode, FileMode::DIR);
        while self.peek()?.map(|next| next.path().starts_with(tree_entry.path)).unwrap_or(false) {
            if let TreeIteratorEntry::File(mut entry) = self.next()?.unwrap() {
                entry.path = tree_entry.path.join(entry.path);
                container.push(entry);
            }
        }
        Ok(())
    }

    // seems difficult to provide a peek method just via an adaptor
    // unclear how to implement peek in terms of `over` and `next`
    // in particular for the case of `TreeIter`,
    // if `peek` uses `next`, then all the subdirectories would already
    // be added to the stack and its awkward to implement `over` after `peek`
    // similar problems arise with implementing `peek` using `over`
    // probably better to just let the implementor deal with it
    // especially as the implementation is probably trivial
    fn peek(&mut self) -> BitResult<Option<Self::Item>>;
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

impl<'r> BitRepo<'r> {
    pub fn head_tree_iter(self) -> BitResult<TreeIter<'r>> {
        let tree = self.head_tree()?;
        Ok(self.tree_iter(&tree))
    }

    pub fn tree_iter(self, tree: &Tree) -> TreeIter<'r> {
        TreeIter::new(self, tree)
    }
}

// TODO do we really need this, or can we just use bitindexentries throughout and just check the mode
// its what's happening a lot of the place already
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeIteratorEntry {
    Tree(TreeEntry),
    File(BitIndexEntry),
}

impl From<TreeEntry> for TreeIteratorEntry {
    fn from(entry: TreeEntry) -> Self {
        match entry.mode {
            FileMode::DIR => Self::Tree(entry),
            FileMode::REG | FileMode::EXEC | FileMode::LINK => Self::File(entry.into()),
            _ => todo!(),
        }
    }
}

impl BitOrd for TreeIteratorEntry {
    fn bit_cmp(&self, other: &Self) -> std::cmp::Ordering {
        // TODO we can probably use the fact that files come before trees and order them this way
        match (self, other) {
            (TreeIteratorEntry::Tree(a), TreeIteratorEntry::Tree(b)) => a.bit_cmp(b),
            (TreeIteratorEntry::Tree(tree), TreeIteratorEntry::File(entry)) =>
                tree.path.cmp(&entry.path).then_with(|| panic!("todo")),
            (TreeIteratorEntry::File(entry), TreeIteratorEntry::Tree(tree)) =>
                entry.path.cmp(&tree.path).then_with(|| panic!("todo")),
            (TreeIteratorEntry::File(a), TreeIteratorEntry::File(b)) => a.bit_cmp(b),
        }
    }
}

impl BitEntry for TreeIteratorEntry {
    fn path(&self) -> BitPath {
        match self {
            TreeIteratorEntry::Tree(tree) => tree.path,
            TreeIteratorEntry::File(entry) => entry.path,
        }
    }

    fn oid(&self) -> Oid {
        match self {
            TreeIteratorEntry::Tree(tree) => tree.oid,
            TreeIteratorEntry::File(entry) => entry.oid,
        }
    }

    fn mode(&self) -> FileMode {
        match self {
            TreeIteratorEntry::Tree(tree) => tree.mode,
            TreeIteratorEntry::File(entry) => entry.mode,
        }
    }
}

#[derive(Debug)]
pub struct TreeIter<'r> {
    repo: BitRepo<'r>,
    // tuple of basepath (the current path up to but not including the path of the entry) and the entry itself
    entry_stack: Vec<(BitPath, TreeEntry)>,
    // the entries of the directory that was just yielded
    // if stepped over, then these are dropped, otherwise they are added to the stack
    dir_entries: Option<Vec<(BitPath, TreeEntry)>>,
}

impl<'r> TreeIter<'r> {
    pub fn new(repo: BitRepo<'r>, tree: &Tree) -> Self {
        Self {
            repo,
            dir_entries: None,
            entry_stack: tree
                .entries
                .iter()
                .cloned()
                .rev()
                .map(|entry| (BitPath::EMPTY, entry))
                .collect(),
        }
    }
}
impl<'r> FallibleIterator for TreeIter<'r> {
    type Error = BitGenericError;
    type Item = TreeIteratorEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(entries) = self.dir_entries.take() {
            self.entry_stack.extend(entries)
        }

        loop {
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    FileMode::DIR => {
                        let tree = self.repo.read_obj(entry.oid)?.into_tree()?;
                        let path = base.join(entry.path);
                        debug!("TreeIter::next: read directory `{:?}` `{}`", path, entry.oid);

                        let entries =
                            tree.entries.into_iter().rev().map(|entry| (path, entry)).collect();
                        debug_assert!(self.dir_entries.is_none());
                        self.dir_entries = Some(entries);
                        return Ok(Some(TreeIteratorEntry::Tree(TreeEntry { path, ..entry })));
                    }
                    FileMode::REG | FileMode::LINK | FileMode::EXEC => {
                        debug!("TreeIter::next: entry: {:?}", entry);
                        entry.path = base.join(entry.path);
                        return Ok(Some(TreeIteratorEntry::File(entry.into())));
                    }
                    // ignore submodules for now
                    FileMode::GITLINK => continue,
                    _ => unreachable!("found unknown filemode `{}`", entry.mode),
                },
                None => return Ok(None),
            }
        }
    }
}
impl<'r> BitTreeIterator for TreeIter<'r> {
    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        match self.next()? {
            Some(entry) => {
                if entry.mode() == FileMode::DIR {
                    self.dir_entries.take();
                }
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        Ok(self.entry_stack.last().map(|x| x.1.into()))
    }
}

pub struct IndexTreeIter<'a, 'r> {
    index: &'a BitIndex<'r>,
    entry_iter: Peekable<IndexEntryIterator>,
    // pseudotrees that have been yielded
    pseudotrees: HashSet<BitPath>,
    peeked: Option<TreeIteratorEntry>,
}

impl<'a, 'r> IndexTreeIter<'a, 'r> {
    pub fn new(index: &'a BitIndex<'r>) -> Self {
        Self {
            index,
            peeked: None,
            entry_iter: index.iter().peekable(),
            pseudotrees: Default::default(),
        }
    }

    fn create_pseudotree(&self, path: BitPath) -> TreeIteratorEntry {
        let oid = self
            .index
            .tree_cache()
            .and_then(|cache| cache.find_valid_child(path))
            .map(|child| child.oid)
            .unwrap_or(Oid::UNKNOWN);
        TreeIteratorEntry::Tree(TreeEntry { mode: FileMode::DIR, path, oid })
    }

    fn step_over_tree(&mut self, tree_entry: TreeEntry) -> BitResult<Option<TreeIteratorEntry>> {
        if let Some(tree_cache) =
            self.index.tree_cache().and_then(|cache| cache.find_valid_child(tree_entry.path))
        {
            // for `n` entries we want to call next `n` times which is what `nth(n-1)` will do
            // we must call `nth` on the inner iterator as that is what `entry_count` corresponds to
            self.entry_iter.nth(tree_cache.entry_count as usize - 1)?;
        } else {
            // step over this tree by stepping over each of its children
            while self.peek()?.map(|next| next.path().starts_with(tree_entry.path)).unwrap_or(false)
            {
                self.over()?;
            }
        }
        Ok(Some(TreeIteratorEntry::Tree(tree_entry)))
    }
}

impl<'a, 'r> FallibleIterator for IndexTreeIter<'a, 'r> {
    type Error = BitGenericError;
    type Item = TreeIteratorEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        // we do want to yield the root tree
        if self.pseudotrees.insert(BitPath::EMPTY) {
            return Ok(Some(self.create_pseudotree(BitPath::EMPTY)));
        }

        if let Some(peeked) = self.peeked.take() {
            return Ok(Some(peeked));
        }

        match self.entry_iter.peek()? {
            Some(&entry) => {
                let dir = entry.path.parent().unwrap();
                if self.pseudotrees.insert(dir) {
                    Ok(Some(self.create_pseudotree(dir)))
                } else {
                    self.entry_iter.next()?;
                    Ok(Some(TreeIteratorEntry::File(entry)))
                }
            }
            None => return Ok(None),
        }
    }
}

impl<'a, 'r> BitTreeIterator for IndexTreeIter<'a, 'r> {
    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        if let Some(peeked) = self.peeked {
            Ok(Some(peeked))
        } else {
            self.peeked = self.next()?;
            Ok(self.peeked)
        }
    }

    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        match self.next()? {
            Some(entry) => match entry {
                TreeIteratorEntry::Tree(tree) => self.step_over_tree(tree),
                TreeIteratorEntry::File(entry) => Ok(Some(TreeIteratorEntry::File(entry))),
            },
            None => return Ok(None),
        }
    }
}
