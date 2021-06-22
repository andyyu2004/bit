use super::*;

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait BitTreeIterator: BitIterator<BitIndexEntry> {
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
        // debug_assert_eq!(tree_entry.mode(), FileMode::TREE);
        while self.peek()?.map(|next| next.path().starts_with(tree_entry.path())).unwrap_or(false) {
            let entry = self.next()?.unwrap();
            if entry.is_file() {
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
        debug_assert!(oid.is_unknown() || repo.read_obj(oid).unwrap().is_tree());
        // if the `oid` is unknown then we just want an empty iterator
        let entry_stack = if oid.is_known() {
            vec![(BitPath::EMPTY, TreeEntry { oid, path: BitPath::EMPTY, mode: FileMode::TREE })]
        } else {
            vec![]
        };
        Self { repo, previous_len: 0, entry_stack }
    }
}

impl<'r> FallibleIterator for TreeIter<'r> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    FileMode::TREE => {
                        let tree = self.repo.read_obj(entry.oid)?.into_tree()?;
                        let path = base.join(entry.path);
                        debug!("TreeIter::next: read directory `{:?}` `{}`", path, entry.oid);

                        self.previous_len = self.entry_stack.len();
                        self.entry_stack.extend(
                            tree.entries
                                .iter()
                                .copied()
                                .rev()
                                // TODO we have to filter out here for now otherwise peek may blow up
                                .filter(|entry| entry.mode != FileMode::GITLINK)
                                .map(|entry| (path, entry)),
                        );

                        return Ok(Some(TreeEntry { path, ..entry }.into()));
                    }
                    FileMode::REG | FileMode::LINK | FileMode::EXEC => {
                        debug!("TreeIter::next: entry: {:?}", entry);
                        entry.path = base.join(entry.path);
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
impl<'r> BitTreeIterator for TreeIter<'r> {
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
        Ok(self.entry_stack.last().map(|(base, mut entry)| {
            entry.path = base.join(entry.path);
            entry.into()
        }))
    }
}
