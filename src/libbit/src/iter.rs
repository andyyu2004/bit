use crate::error::{BitGenericError, BitResult};
use crate::index::BitIndexEntry;
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

        let path = tls::REPO.with(|repo| repo.to_relative_path(direntry.path()))?;
        BitIndexEntry::try_from(path).map(Some)
    }
}

pub trait BitIterator = FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>;

impl BitRepo {
    pub fn worktree_iter(&self) -> impl BitIterator {
        WorktreeIter::new(&self.worktree)
    }
}

trait BitIteratorExt: BitIterator {}

impl<I: BitIterator> BitIteratorExt for I {
}
