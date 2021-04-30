use crate::error::{BitGenericError, BitResult};
use crate::index::BitIndexEntry;
use crate::obj::Tree;
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use ignore::{Walk, WalkBuilder};
use std::convert::TryFrom;
use std::path::Path;

struct WorktreeIter<'r> {
    repo: &'r BitRepo,
    walk: Walk,
}

impl<'r> WorktreeIter<'r> {
    pub fn new(repo: &'r BitRepo) -> BitResult<Self> {
        Ok(Self {
            repo,
            walk: WalkBuilder::new(repo.workdir).sort_by_file_path(Ord::cmp).hidden(false).build(),
        })
    }

    // we need to explicitly ignore our root `.bit/.git` directories
    fn ignored(&self, path: &Path) -> BitResult<bool> {
        let path = self.repo.to_relative_path(path)?;
        let fst_component = path.components()[0];
        Ok(fst_component == ".bit" || fst_component == ".git")
    }
}

impl FallibleIterator for WorktreeIter<'_> {
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

struct HeadIter<'r> {
    repo: &'r BitRepo,
    root: Tree,
}

impl<'r> FallibleIterator for HeadIter<'r> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}

pub trait BitIterator = FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>;

impl BitRepo {
    pub fn worktree_iter(&self) -> BitResult<impl BitIterator + '_> {
        let mut entries: Vec<_> = WorktreeIter::new(self)?.collect()?;
        // TODO worktree iterator does not return in the correct order
        // the comparator function on works per directory
        // for some reason git places files before directory
        // i.e. src/index.rs < index/mod.rs
        // but no directory I've seen does this so we just collect and sort for now
        entries.sort();
        Ok(fallible_iterator::convert(entries.into_iter().map(Ok)))
    }

    pub fn head_iter(&self) -> BitResult<impl BitIterator + '_> {
        let head = self.read_head()?.expect("todo, what to do if no head yet");
        let hash = head.resolve(self)?;
        let root = self.read_obj(hash)?.into_tree();
        Ok(HeadIter { repo: self, root })
    }
}

trait BitIteratorExt: BitIterator {}

impl<I: BitIterator> BitIteratorExt for I {
}
