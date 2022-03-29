use super::*;

pub struct WorktreeTreeIter {
    inner: WorktreeRawIter,
    peeked: Option<BitIndexEntry>,
}

impl WorktreeTreeIter {
    pub fn new(index: &BitIndex) -> BitResult<Self> {
        Ok(Self { inner: WorktreeRawIter::new(index)?, peeked: None })
    }
}

impl FallibleIterator for WorktreeTreeIter {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(peeked) = self.peeked.take() {
            return Ok(Some(peeked));
        }

        let entry = match self.inner.next()? {
            Some(entry) => entry,
            None => return Ok(None),
        };

        let repo = self.inner.repo;
        let index_entry = if entry.file_type.is_dir() {
            TreeEntry {
                oid: Oid::UNKNOWN,
                mode: FileMode::TREE,
                path: BitPath::intern(repo.to_relative_path(&entry.path)?),
            }
            .into()
        } else {
            BitIndexEntry::from_absolute_path(&repo, &entry.path)?
        };
        Ok(Some(index_entry))
    }
}

impl BitTreeIterator for WorktreeTreeIter {
    fn kind(&self) -> IterKind {
        IterKind::Worktree
    }

    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        let mut iter = self.as_consumer().iter();
        let tree_entry = iter.next();
        iter.count()?;
        tree_entry
    }

    // these copy/paste manual peek impls are a bit sad but not sure how to avoid them
    fn peek(&mut self) -> BitResult<Option<Self::Item>> {
        if let Some(peeked) = self.peeked {
            Ok(Some(peeked))
        } else {
            self.peeked = self.next()?;
            Ok(self.peeked)
        }
    }
}
