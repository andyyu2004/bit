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
    pub fn new(root: BitPath) -> BitResult<Self> {
        Ok(Self { walk: WalkBuilder::new(root).sort_by_file_path(Ord::cmp).hidden(false).build() })
    }

    // we need to explicitly ignore our root `.bit/.git` directories
    fn ignored(&self, path: &Path) -> BitResult<bool> {
        let path = tls::REPO.with(|repo| repo.to_relative_path(path))?;
        let fst_component = path.components()[0];
        Ok(fst_component == ".bit" || fst_component == ".git")
    }
}

impl FallibleIterator for WorktreeIter {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        // ignore directories
        let direntry = loop {
            match self.walk.next().transpose()? {
                Some(entry) => {
                    let path = entry.path();
                    if path.is_file() && !self.ignored(path)? {
                        break entry;
                    }
                }
                None => return Ok(None),
            }
        };

        BitIndexEntry::try_from(BitPath::intern(direntry.path())).map(Some)
    }
}

pub trait BitIterator = FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>;

impl BitRepo {
    pub fn worktree_iter(&self) -> BitResult<impl BitIterator> {
        WorktreeIter::new(self.workdir)
    }
}

trait BitIteratorExt: BitIterator {}

impl<I: BitIterator> BitIteratorExt for I {
}
