use super::*;

pub struct WorktreeTreeIter<'rcx> {
    inner: WorktreeRawIter<'rcx>,
    peeked: Option<BitIndexEntry>,
}

impl<'rcx> WorktreeTreeIter<'rcx> {
    pub fn new(index: &BitIndex<'rcx>) -> BitResult<Self> {
        Ok(Self { inner: WorktreeRawIter::new(index)?, peeked: None })
    }
}

impl<'rcx> FallibleIterator for WorktreeTreeIter<'rcx> {
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
        let path = BitPath::intern(repo.to_relative_path(entry.path())?);
        let oid = if entry.file_type().is_file() { repo.hash_blob(path)? } else { Oid::UNKNOWN };
        let tree_entry = TreeEntry { path, oid, mode: FileMode::from_metadata(&entry.metadata()?) };

        Ok(Some(tree_entry.into()))
    }
}

impl<'rcx> BitTreeIterator for WorktreeTreeIter<'rcx> {
    fn over(&mut self) -> BitResult<Option<Self::Item>> {
        let entry = match self.next()? {
            Some(entry) => entry,
            None => return Ok(None),
        };

        if entry.is_tree() {
            self.inner.skip_current_dir();
        }

        Ok(Some(entry))
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