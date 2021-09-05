mod index_tree_iter;
mod tree_iter;
mod walk;
mod worktree_tree_iter;

pub use fallible_iterator::FallibleIterator;
pub use index_tree_iter::IndexTreeIter;
pub use tree_iter::*;
pub use worktree_tree_iter::WorktreeTreeIter;

use crate::error::{BitErrorExt, BitGenericError, BitResult};
use crate::index::{BitIndex, BitIndexEntry, IndexEntryIterator};
use crate::obj::{FileMode, MutableBlob, Oid, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::Peekable;
use ignore::gitignore::Gitignore;
use ignore::{Walk, WalkBuilder};
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::FileType;
use std::path::{Path, PathBuf};

pub type PathMode = (BitPath, FileMode);

pub trait BitEntry {
    fn oid(&self) -> Oid;
    fn path(&self) -> BitPath;
    fn mode(&self) -> FileMode;

    fn path_mode(&self) -> PathMode {
        (self.path(), self.mode())
    }

    /// Comparison function for differs
    // This is not an `Ord` impl as it doesn't satisfy the `Ord` invariant `a.cmp(b) == Ordering::Equal <=> a == b`
    fn entry_cmp(&self, other: &Self) -> Ordering {
        self.path().cmp(&other.path())
    }

    fn entry_partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.entry_cmp(other))
    }

    fn is_tree(&self) -> bool {
        self.mode().is_tree()
    }

    fn is_blob(&self) -> bool {
        self.mode().is_blob()
    }

    fn is_gitlink(&self) -> bool {
        self.mode().is_gitlink()
    }

    fn write(&self, repo: BitRepo<'_>) -> BitResult<Oid> {
        let oid = self.oid();
        if oid.is_known() {
            return Ok(oid);
        }
        let blob = self.read_to_blob(repo)?;
        repo.write_obj(&blob)
    }

    fn read_to_blob(&self, repo: BitRepo<'_>) -> BitResult<MutableBlob> {
        let oid = self.oid();
        // if object is known we try to read it from the object store
        // however, it's possible the object does not live there as the hash may have just been calculated to allow for comparisons
        // if it's not in the object store, then it must live on disk so we just read it from there
        // if the oid is not known, then it's definitely on disk (as otherwise it would have a known `oid`)
        if oid.is_known() {
            match repo.read_obj(oid) {
                Ok(obj) => return Ok(obj.into_blob().clone().into_inner()),
                Err(err) => err.try_into_obj_not_found_err()?,
            };
        }

        repo.read_blob_from_worktree(self.path())
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
                Some(entry) if entry.is_blob() => return Ok(Some(entry)),
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

/// Iterator that yields filesystem entries accounting for gitignore and tracked files
// Intended to be used as a building block for higher level worktree iterators
struct WorktreeRawIter<'rcx> {
    repo: BitRepo<'rcx>,
    // this is significantly faster than using bitpath as key for some reason
    tracked: FxHashSet<&'static OsStr>,
    // TODO ignoring all nonroot ignores for now
    // not sure what the correct collection for this is? some kind of tree where gitignores know their "parent" gitignore?
    ignore: Vec<Gitignore>,
    jwalk: jwalk::DirEntryIter<((), ())>,
}

impl<'rcx> WorktreeRawIter<'rcx> {
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
            jwalk: jwalk::WalkDir::new(repo.workdir)
                .skip_hidden(false)
                .process_read_dir(|_, _, _, children| {
                    // ignore `.git` directories
                    children.retain(|entry| {
                        entry
                            .as_ref()
                            .map(|entry| BitPath::DOT_GIT != entry.file_name())
                            .unwrap_or(true)
                    });
                    children.sort_by(|a, b| match (a, b) {
                        (Ok(a), Ok(b)) => {
                            let mut x = a.path();
                            let mut y = b.path();
                            // for correct ordering and avoiding allocation where possible
                            if a.file_type().is_dir() {
                                x.push(BitPath::EMPTY);
                            }
                            if b.file_type().is_dir() {
                                y.push(BitPath::EMPTY);
                            }
                            BitPath::path_cmp(x, y)
                        }
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        (Err(_), Err(_)) => Ordering::Equal,
                    })
                })
                .into_iter(),
        })
    }

    // we need to explicitly ignore our root `.bit/.git` directories
    // TODO testing
    fn is_ignored(&self, entry: &DirEntry) -> BitResult<bool> {
        debug_assert!(entry.path.is_absolute());

        let relative = self.repo.to_relative_path(&entry.path)?;
        debug_assert!(
            relative.iter().all(|component| BitPath::DOT_GIT != component),
            "git directories should be filtered out by now"
        );

        if self.tracked.contains(&relative.as_os_str()) {
            return Ok(false);
        }

        for ignore in &self.ignore {
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
            if ignore.matched_path_or_any_parents(&entry.path, entry.file_type.is_dir()).is_ignore()
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[derive(Debug)]
pub struct DirEntry {
    file_type: FileType,
    path: PathBuf,
}

impl FallibleIterator for WorktreeRawIter<'_> {
    type Error = BitGenericError;
    type Item = DirEntry;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        loop {
            match self.jwalk.next().transpose()? {
                Some(entry) => {
                    let entry = DirEntry { path: entry.path(), file_type: entry.file_type() };
                    if !self.is_ignored(&entry)? {
                        // this iterator doesn't yield directory entries
                        return Ok(Some(entry));
                    }
                }
                None => return Ok(None),
            }
        }
    }
}

pub struct WorktreeIter<'rcx> {
    inner: WorktreeRawIter<'rcx>,
}

impl<'rcx> WorktreeIter<'rcx> {
    pub fn new(index: &BitIndex<'rcx>) -> BitResult<Self> {
        Ok(Self { inner: WorktreeRawIter::new(index)? })
    }
}

impl FallibleIterator for WorktreeIter<'_> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        loop {
            let entry = match self.inner.next()? {
                Some(path) => path,
                None => return Ok(None),
            };

            // we don't yield directory entries in this type of iterator
            if entry.file_type.is_dir() {
                continue;
            }

            return BitIndexEntry::from_path(self.inner.repo, &entry.path).map(Some);
        }
    }
}

pub trait BitEntryIterator = BitIterator<BitIndexEntry>;

pub trait BitIterator<T> = FallibleIterator<Item = T, Error = BitGenericError>;

impl<'rcx> BitIndex<'rcx> {
    pub fn worktree_iter(&self) -> BitResult<impl BitEntryIterator + 'rcx> {
        trace!("worktree_iter()");
        WorktreeIter::new(self)
    }

    pub fn worktree_tree_iter(&self) -> BitResult<impl BitTreeIterator + 'rcx> {
        trace!("worktree_tree_iter()");
        WorktreeTreeIter::new(self)
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
