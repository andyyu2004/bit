mod index_tree_iter;
mod tree_iter;
mod worktree_tree_iter;

pub use index_tree_iter::IndexTreeIter;
pub use tree_iter::*;
pub use worktree_tree_iter::WorktreeTreeIter;

use crate::error::{BitGenericError, BitResult};
use crate::index::{BitIndex, BitIndexEntry, IndexEntryIterator};
use crate::obj::{FileMode, Oid, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::{FallibleIterator, Peekable};
use ignore::gitignore::Gitignore;
use ignore::{Walk, WalkBuilder};
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;
use walkdir::WalkDir;

pub trait BitEntry {
    fn oid(&self) -> Oid;
    fn path(&self) -> BitPath;
    fn mode(&self) -> FileMode;

    fn is_tree(&self) -> bool {
        self.mode().is_tree()
    }

    fn is_file(&self) -> bool {
        self.mode().is_file()
    }

    // we must have files sorted before directories
    // i.e. index.rs < index/
    // however, the trailing slash is not actually stored in the tree entry path (TODO confirm against git)
    // we fix this by appending appending a slash
    fn sort_path(&self) -> Cow<'static, Path> {
        if self.mode() == FileMode::TREE {
            Cow::Owned(self.path().join_trailing_slash())
        } else {
            Cow::Borrowed(self.path().as_path())
        }
    }
}

/// wrapper around `TreeIter` that skips the tree entries
#[derive(Debug)]
pub struct TreeEntryIter<'rcx> {
    tree_iter: TreeIter<'rcx>,
}

impl<'rcx> TreeEntryIter<'rcx> {
    pub fn new(repo: BitRepo<'rcx>, oid: Oid) -> Self {
        Self { tree_iter: TreeIter::new(repo, oid) }
    }
}

impl<'rcx> FallibleIterator for TreeEntryIter<'rcx> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        // entry iterators only yield non-tree entries
        loop {
            match self.tree_iter.next()? {
                Some(entry) if entry.is_file() => return Ok(Some(BitIndexEntry::from(entry))),
                None => return Ok(None),
                _ => continue,
            }
        }
    }
}

pub struct DirIter {
    walk: Walk,
}

impl DirIter {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { walk: WalkBuilder::new(path).sort_by_file_path(Ord::cmp).build() }
    }
}

impl FallibleIterator for DirIter {
    type Error = BitGenericError;
    type Item = ignore::DirEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let entry = self.walk.next().transpose()?;
        match entry {
            Some(entry) => Ok(Some(entry)),
            None => Ok(None),
        }
    }
}

struct WorktreeIter<'rcx> {
    repo: BitRepo<'rcx>,
    tracked: FxHashSet<&'static OsStr>,
    // TODO ignoring all nonroot ignores for now
    // not sure what the correct collection for this is? some kind of tree where gitignores know their "parent" gitignore?
    ignore: Vec<Gitignore>,
    walk: walkdir::IntoIter,
}

impl<'rcx> WorktreeIter<'rcx> {
    pub fn new(index: &BitIndex<'rcx>) -> BitResult<Self> {
        let repo = index.repo;
        // ignoring any gitignore errors for now
        let ignore = vec![Gitignore::new(repo.workdir.join(".gitignore").as_path()).0];
        //? we collect it into a hashmap for faster lookup?
        // not sure if this is actually better than just looking up in the index's entries
        let tracked = index.entries().keys().map(|(path, _)| path.as_os_str()).collect();

        Ok(Self {
            repo,
            ignore,
            tracked,
            walk: WalkDir::new(repo.workdir)
                .sort_by(|a, b| {
                    let x = a.path();
                    let y = b.path();

                    // TODO should be able to rewrite this without the match now
                    //  for ordering and avoiding allocation where possible
                    let t = a.file_type().is_dir().then(|| x.join(""));
                    let u = b.file_type().is_dir().then(|| y.join(""));

                    match (&t, &u) {
                        (None, None) => BitPath::path_cmp(x, y),
                        (None, Some(u)) => BitPath::path_cmp(x, u),
                        (Some(t), None) => BitPath::path_cmp(t, y),
                        (Some(t), Some(u)) => BitPath::path_cmp(t, u),
                    }
                })
                .into_iter(),
        })
    }

    // we need to explicitly ignore our root `.bit/.git` directories
    // TODO testing
    fn is_ignored(&self, path: &Path) -> BitResult<bool> {
        // should only run this on files?
        debug_assert!(path.is_file());
        debug_assert!(path.is_absolute());
        let relative = self.repo.to_relative_path(path)?;

        if self.tracked.contains(&relative.as_os_str()) {
            return Ok(false);
        }

        // not ignoring .bit as git doesn't ignore .bit
        // perhaps we should just consider .bit as a debug directory only?
        if relative.iter().any(|component| component == BitPath::DOT_GIT.as_os_str()) {
            return Ok(true);
        }

        for ignore in &self.ignore {
            // TODO we need to not ignore files that are already tracked
            // where a file is tracked if its in the index
            // we need a different api for the index
            // the with_index api is not good

            // TODO the matcher wants a path relative to where the gitignore file is
            // currently just assuming its the repo root (which all paths are relative to)
            // `ignore.matched_path` doesn't work here as it doesn't seem to match directories
            // e.g.
            // tree! {
            //     ignoreme {
            //         a
            //     }
            // }
            //
            // gitignore! {
            //     ignoreme
            // }
            // path ignoreme/a will not be ignored by `matched_path`
            // using `matched_path_or_any_parents` for now
            if ignore.matched_path_or_any_parents(path, false).is_ignore() {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

impl FallibleIterator for WorktreeIter<'_> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        // ignore directories
        let direntry = loop {
            // TODO can we ignore .git, its a waste of time travering that directory just to be ignored
            match self.walk.next().transpose()? {
                Some(entry) => {
                    let path = entry.path();
                    let is_dir = path.is_dir();
                    if !is_dir && !self.is_ignored(path)? {
                        break entry;
                    }
                }
                None => return Ok(None),
            }
        };

        BitIndexEntry::from_path(self.repo, direntry.path()).map(Some)
    }
}

pub trait BitEntryIterator = BitIterator<BitIndexEntry>;

pub trait BitIterator<T> = FallibleIterator<Item = T, Error = BitGenericError>;

impl<'rcx> BitIndex<'rcx> {
    pub fn worktree_iter(&self) -> BitResult<impl BitEntryIterator + 'rcx> {
        trace!("worktree_iter()");
        WorktreeIter::new(&self)
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn tree_entry_iter(self, oid: Oid) -> BitResult<impl BitEntryIterator + 'rcx> {
        trace!("tree_entry_iter(oid: {})", oid);
        Ok(TreeEntryIter::new(self, oid))
    }

    pub fn head_iter(self) -> BitResult<impl BitEntryIterator + 'rcx> {
        trace!("head_iter()");
        let oid = self.head_tree()?;
        self.tree_entry_iter(oid)
    }
}

trait BitIteratorExt: BitEntryIterator {}

impl<I: BitEntryIterator> BitIteratorExt for I {
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_iter_tests;
