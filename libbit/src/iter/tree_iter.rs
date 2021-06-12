use super::*;
use crate::core::BitOrd;

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait BitTreeIterator: BitIterator<TreeIteratorEntry> {
    /// unstable semantics
    /// if the next entry is a tree then yield the tree entry but skip over its contents
    /// otherwise does the same as next
    /// `next` should always yield the tree entry itself
    /// if the `peeked` entry is a directory and over is called, then that entry should be yielded
    /// and its contents skipped over
    /// full paths should be returned (relative to repo root), not just relative to the parent
    fn over(&mut self) -> BitResult<Option<Self::Item>>;

    /// same as `self.over` but instead appends all the non-tree entries into a container
    // this takes a container to append to instead of returning a vec to avoid a separate allocation
    fn collect_over_tree(&mut self, container: &mut Vec<BitIndexEntry>) -> BitResult<()> {
        let tree_entry = self.peek()?.expect("currently expected to not be called when at end");
        // debug_assert_eq!(tree_entry.mode(), FileMode::DIR);
        while self.peek()?.map(|next| next.path().starts_with(tree_entry.path())).unwrap_or(false) {
            if let TreeIteratorEntry::File(entry) = self.next()?.unwrap() {
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
        let oid = self.head_tree_oid()?;
        Ok(self.tree_iter(oid))
    }

    /// return's tree iterator for a tree with `oid`
    pub fn tree_iter(self, oid: Oid) -> TreeIter<'r> {
        TreeIter::new(self, oid)
    }
}

// TODO do we really need this, or can we just use bitindexentries throughout and just check the mode
// as its what's happening a lot of the place already
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeIteratorEntry {
    Tree(TreeEntry),
    File(BitIndexEntry),
}

impl From<TreeEntry> for TreeIteratorEntry {
    fn from(entry: TreeEntry) -> Self {
        match entry.mode {
            FileMode::DIR => Self::Tree(entry),
            mode if mode.is_file() => Self::File(entry.into()),
            _ => todo!(),
        }
    }
}

impl TreeIteratorEntry {
    fn sort_path(&self) -> BitPath {
        if self.mode() == FileMode::DIR { self.path().join_trailing_slash() } else { self.path() }
    }
}

impl BitOrd for TreeIteratorEntry {
    fn bit_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_path().cmp(&other.sort_path())
        // match (self, other) {
        //     (TreeIteratorEntry::Tree(a), TreeIteratorEntry::Tree(b)) => a.bit_cmp(b),
        //     // TODO using the fact that files come before trees of the same name?
        //     // or should they just be considered equal?
        //     (TreeIteratorEntry::Tree(tree), TreeIteratorEntry::File(entry)) =>
        //         tree.path.cmp(&entry.path).then(std::cmp::Ordering::Greater),
        //     (TreeIteratorEntry::File(entry), TreeIteratorEntry::Tree(tree)) =>
        //         entry.path.cmp(&tree.path).then(std::cmp::Ordering::Less),
        //     (TreeIteratorEntry::File(a), TreeIteratorEntry::File(b)) => a.bit_cmp(b),
        // }
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
    /// the number of entries in the stack before the most recent directory was pushed
    /// this is used for stepping over
    previous_len: usize,
}

impl<'r> TreeIter<'r> {
    pub fn new(repo: BitRepo<'r>, oid: Oid) -> Self {
        // if the `oid` is unknown then we just want an empty iterator
        let entry_stack = if oid.is_known() {
            vec![(BitPath::EMPTY, TreeEntry { oid, path: BitPath::EMPTY, mode: FileMode::DIR })]
        } else {
            vec![]
        };
        Self { repo, previous_len: 0, entry_stack }
    }
}
impl<'r> FallibleIterator for TreeIter<'r> {
    type Error = BitGenericError;
    type Item = TreeIteratorEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    FileMode::DIR => {
                        let tree = self.repo.read_obj(entry.oid)?.into_tree()?;
                        let path = base.join(entry.path);
                        debug!("TreeIter::next: read directory `{:?}` `{}`", path, entry.oid);

                        self.previous_len = self.entry_stack.len();
                        self.entry_stack.extend(
                            tree.entries
                                .into_iter()
                                .rev()
                                // TODO we have to filter out here for now otherwise peek may blow up
                                .filter(|entry| entry.mode != FileMode::GITLINK)
                                .map(|entry| (path, entry)),
                        );

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
        // TODO does this truncate to previous_len technique always work?
        // seems dodgy but haven't found a counterexample yet
        match self.next()? {
            Some(entry) => {
                if entry.mode() == FileMode::DIR {
                    self.entry_stack.truncate(self.previous_len);
                }
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        Ok(self.entry_stack.last().map(|(base, mut entry)| {
            entry.path = base.join(entry.path);
            entry.into()
        }))
    }
}
