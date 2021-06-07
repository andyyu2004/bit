use super::*;

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait BitTreeIterator: BitIterator<TreeEntry> {
    /// unstable semantics
    /// if the next entry is a tree then yield the tree entry but skip over its contents
    /// otherwise does the same as next
    /// `next` should always yield the tree entry itself
    /// if the `peeked` entry is a directory and over is called, then that entry should be yielded
    /// and its contents skipped over
    fn over(&mut self) -> BitResult<Option<TreeEntry>>;

    // seems difficult to provide a peek method just via an adaptor
    // unclear how to implement peek in terms of `over` and `next`
    // in particular for the case of `TreeIter`,
    // if `peek` uses `next`, then all the subdirectories would already
    // be added to the stack and its awkward to implement `over` after `peek`
    // similar problems arise with implementing `peek` using `over`
    // probably better to just let the implementor deal with it
    // especially as the implementation is probably trivial
    fn peek(&mut self) -> BitResult<Option<TreeEntry>>;
}

impl<I, F> BitTreeIterator for fallible_iterator::Filter<I, F>
where
    I: BitTreeIterator,
    F: FnMut(&I::Item) -> Result<bool, I::Error>,
{
    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        let mut next = self.it.over()?;
        while let Some(x) = next {
            if (self.f)(&x)? {
                break;
            }
            next = self.it.over()?;
        }
        Ok(next)
    }

    fn peek(&mut self) -> BitResult<Option<TreeEntry>> {
        loop {
            let peeked = self.it.peek()?;
            if let Some(x) = peeked {
                if (self.f)(&x)? {
                    break;
                }
            }
            self.it.next()?;
        }
        self.it.peek()
    }
}

impl<'r> BitTreeIterator for TreeIter<'r> {
    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        match self.next()? {
            Some(entry) => {
                if entry.mode == FileMode::DIR {
                    self.dir_entries.take();
                }
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn peek(&mut self) -> BitResult<Option<TreeEntry>> {
        Ok(self.entry_stack.last().map(|x| x.1))
    }
}

pub struct IndexTreeIter<'a, 'r> {
    index: &'a BitIndex<'r>,
    entry_iter: Peekable<IndexEntryIterator>,
    // pseudotrees that have been yielded
    pseudotrees: HashSet<BitPath>,
    peeked: Option<TreeEntry>,
}

impl<'a, 'r> IndexTreeIter<'a, 'r> {
    pub fn new(index: &'a BitIndex<'r>) -> Self {
        Self {
            index,
            peeked: None,
            entry_iter: index.iter().peekable(),
            pseudotrees: hashset! { BitPath::EMPTY },
        }
    }

    fn create_pseudotree(&self, path: BitPath) -> TreeEntry {
        let oid = self
            .index
            .tree_cache()
            .and_then(|cache| cache.find_valid_child(path))
            .map(|child| child.oid)
            .unwrap_or(Oid::UNKNOWN);
        TreeEntry { mode: FileMode::DIR, path, oid }
    }

    fn step_over_dir(&mut self, dir_entry: TreeEntry) -> BitResult<Option<TreeEntry>> {
        if let Some(tree_cache) =
            self.index.tree_cache().and_then(|cache| cache.find_valid_child(dir_entry.path))
        {
            // for `n` entries we want to call next `n` times which is what `nth(n-1)` will do
            // we must call `nth` on the inner iterator as that is what `entry_count` corresponds to
            self.entry_iter.nth(tree_cache.entry_count as usize - 1)?;
        } else {
            // step over this tree by stepping over each of its children
            while self.peek()?.map(|next| next.path.starts_with(dir_entry.path)).unwrap_or(false) {
                self.over()?;
            }
        }
        Ok(Some(dir_entry))
    }
}

impl<'a, 'r> FallibleIterator for IndexTreeIter<'a, 'r> {
    type Error = BitGenericError;
    type Item = TreeEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(peeked) = self.peeked.take() {
            return Ok(Some(peeked));
        }

        let next = self.entry_iter.peek()?.map(TreeEntry::from);
        match next {
            Some(entry) => {
                let dir = entry.path.parent().unwrap();
                if self.pseudotrees.insert(dir) {
                    Ok(Some(self.create_pseudotree(dir)))
                } else {
                    self.entry_iter.next()?;
                    Ok(next)
                }
            }
            None => return Ok(None),
        }
    }
}

impl<'a, 'r> BitTreeIterator for IndexTreeIter<'a, 'r> {
    fn peek(&mut self) -> BitResult<Option<TreeEntry>> {
        if let Some(peeked) = self.peeked {
            Ok(Some(peeked))
        } else {
            self.peeked = self.next()?;
            Ok(self.peeked)
        }
    }

    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        match self.next()? {
            Some(entry) => match entry.mode {
                FileMode::DIR => self.step_over_dir(entry),
                FileMode::REG | FileMode::EXEC | FileMode::LINK => Ok(Some(entry)),
                FileMode::GITLINK => self.over(),
                _ => unreachable!(),
            },
            None => return Ok(None),
        }
    }
}
