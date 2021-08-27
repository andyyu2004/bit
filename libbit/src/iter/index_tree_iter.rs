use super::*;
use crate::index::BitIndexInner;

pub struct IndexTreeIter<'a> {
    index: &'a BitIndexInner,
    entry_iter: Peekable<IndexEntryIterator>,
    // pseudotrees that have been yielded
    pseudotrees: FxHashSet<BitPath>,
    peeked: Option<BitIndexEntry>,
}

impl<'a> IndexTreeIter<'a> {
    pub fn new(index: &'a BitIndexInner) -> Self {
        Self {
            index,
            peeked: None,
            entry_iter: index.iter().peekable(),
            pseudotrees: Default::default(),
        }
    }

    fn create_pseudotree(&self, path: BitPath) -> BitIndexEntry {
        let oid = self
            .index
            .tree_cache()
            .and_then(|cache| cache.find_valid_child(path))
            .map(|child| child.tree_oid)
            .unwrap_or(Oid::UNKNOWN);
        TreeEntry { mode: FileMode::TREE, path, oid }.into()
    }

    fn step_over_tree(&mut self, entry: BitIndexEntry) -> BitResult<Option<BitIndexEntry>> {
        if let Some(tree_cache) =
            self.index.tree_cache().and_then(|cache| cache.find_valid_child(entry.path))
        {
            // for `n` entries we want to call next `n` times which is what `nth(n-1)` will do
            // we must call `nth` on the inner iterator as that is what `entry_count` corresponds to
            self.entry_iter.nth(tree_cache.entry_count as usize - 1)?;
        } else {
            // step over this tree by stepping over each of its children
            while self.peek()?.map(|next| next.path().starts_with(entry.path)).unwrap_or(false) {
                self.over()?;
            }
        }
        Ok(Some(entry))
    }
}

impl<'a> FallibleIterator for IndexTreeIter<'a> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

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
                let dir = entry.path().parent().unwrap();
                // we must check for all parents whether a pseudotree has been yielded for that tree
                for parent in dir.cumulative_components() {
                    if self.pseudotrees.insert(parent) {
                        return Ok(Some(self.create_pseudotree(parent)));
                    }
                }
                self.entry_iter.next()?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }
}

impl<'a> BitTreeIterator for IndexTreeIter<'a> {
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
            Some(entry) => match entry.mode() {
                FileMode::TREE => self.step_over_tree(entry),
                mode if mode.is_blob() => Ok(Some(entry)),
                _ => unreachable!(),
            },
            None => Ok(None),
        }
    }
}
