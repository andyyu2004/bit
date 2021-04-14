use crate::error::{BitGenericError, BitResult};
use crate::index::BitIndexEntry;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::FallibleIterator;
use ignore::{Walk, WalkBuilder};
use std::convert::TryFrom;
use std::path::Path;

struct WorktreeIter {
    walk: Walk,
}

impl WorktreeIter {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { walk: WalkBuilder::new(path).sort_by_file_path(Ord::cmp).build() }
    }
}

impl FallibleIterator for WorktreeIter {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        // ignore directories
        let direntry = loop {
            match self.walk.next().transpose()? {
                Some(entry) =>
                    if entry.path().is_file() {
                        break entry;
                    },
                None => return Ok(None),
            }
        };

        let path = direntry.path();
        BitIndexEntry::try_from(BitPath::intern(path)).map(Some)
    }
}

// an iterator adaptor that converts all absolute paths to relative ones
struct Relative<I: BitIterator> {
    inner: I,
}

pub trait BitIterator = FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>;

trait BitIteratorExt: BitIterator {
    fn relative(self) -> Relative<Self>
    where
        Self: Sized,
    {
        Relative { inner: self }
    }
}

impl<I: BitIterator> BitIteratorExt for I {
}

impl<I> FallibleIterator for Relative<I>
where
    I: BitIterator,
{
    type Error = I::Error;
    type Item = I::Item;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.next()? {
            Some(entry) => {
                let filepath = tls::REPO.with(|repo| repo.to_relative_path(entry.filepath))?;
                Ok(Some(BitIndexEntry { filepath, ..entry }))
            }

            None => Ok(None),
        }
    }
}

impl BitRepo {
    pub fn worktree_iter(&self) -> impl BitIterator {
        WorktreeIter::new(&self.worktree).relative()
    }
}
