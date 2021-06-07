mod tree_iter;

pub use tree_iter::*;

use crate::error::{BitGenericError, BitResult};
use crate::index::{BitIndex, BitIndexEntry, IndexEntryIterator};
use crate::obj::{FileMode, Oid, Tree, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::{FallibleIterator, Peekable};
use ignore::{Walk, WalkBuilder};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::path::Path;
use walkdir::WalkDir;

impl<'r> BitRepo<'r> {
    pub fn head_tree_iter(self) -> BitResult<TreeIter<'r>> {
        let tree = self.head_tree()?;
        Ok(self.tree_iter(&tree))
    }

    pub fn tree_iter(self, tree: &Tree) -> TreeIter<'r> {
        TreeIter::new(self, tree)
    }
}

#[derive(Debug)]
pub struct TreeIter<'r> {
    repo: BitRepo<'r>,
    // tuple of basepath (the current path up to but not including the path of the entry) and the entry itself
    entry_stack: Vec<(BitPath, TreeEntry)>,
    // the entries of the directory that was just yielded
    // if stepped over, then these are dropped, otherwise they are added to the stack
    dir_entries: Option<Vec<(BitPath, TreeEntry)>>,
}

impl<'r> TreeIter<'r> {
    pub fn new(repo: BitRepo<'r>, tree: &Tree) -> Self {
        Self {
            repo,
            dir_entries: None,
            entry_stack: tree
                .entries
                .iter()
                .cloned()
                .rev()
                .map(|entry| (BitPath::EMPTY, entry))
                .collect(),
        }
    }
}
impl<'r> FallibleIterator for TreeIter<'r> {
    type Error = BitGenericError;
    type Item = TreeEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(entries) = self.dir_entries.take() {
            self.entry_stack.extend(entries)
        }

        loop {
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    FileMode::DIR => {
                        let tree = self.repo.read_obj(entry.oid)?.into_tree()?;
                        let path = base.join(entry.path);
                        debug!("TreeIter::next: read directory `{:?}` `{}`", path, entry.oid);

                        let entries =
                            tree.entries.into_iter().rev().map(|entry| (path, entry)).collect();
                        debug_assert!(self.dir_entries.is_none());
                        self.dir_entries = Some(entries);
                        return Ok(Some(TreeEntry { path, ..entry }));
                    }
                    FileMode::REG | FileMode::LINK | FileMode::EXEC => {
                        debug!("TreeIter::next: entry: {:?}", entry);
                        entry.path = base.join(entry.path);
                        return Ok(Some(entry));
                    }
                    // ignore submodules for now
                    FileMode::GITLINK => continue,
                    _ => unreachable!("found unknown filemode `{}`", entry.mode),
                },
                None => return Ok(None),
            }
        }
    }
}

/// wrapper around `TreeIter` that skips the tree entries
#[derive(Debug)]
pub struct TreeEntryIter<'r> {
    tree_iter: TreeIter<'r>,
}

impl<'r> TreeEntryIter<'r> {
    pub fn new(repo: BitRepo<'r>, root: &Tree) -> Self {
        Self { tree_iter: TreeIter::new(repo, root) }
    }
}

impl<'r> FallibleIterator for TreeEntryIter<'r> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        // entry iterators only yield non tree entries
        loop {
            match self.tree_iter.next()? {
                Some(entry) if !entry.mode.is_tree() =>
                    return Ok(Some(BitIndexEntry::from(entry))),
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

struct WorktreeIter<'r> {
    repo: BitRepo<'r>,
    walk: walkdir::IntoIter,
}

impl<'r> WorktreeIter<'r> {
    pub fn new(repo: BitRepo<'r>) -> BitResult<Self> {
        Ok(Self {
            repo,
            walk: WalkDir::new(repo.workdir)
                .sort_by(|a, b| {
                    let x = a.path().to_str().unwrap();
                    let y = b.path().to_str().unwrap();

                    //  for ordering and avoiding allocation where possible
                    let t = a.file_type().is_dir().then(|| x.to_owned() + "/");
                    let u = b.file_type().is_dir().then(|| y.to_owned() + "/");

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
        // does this not have an iterator api??? yikers
        let direntry = loop {
            match self.walk.next().transpose()? {
                Some(entry) => {
                    let path = entry.path();
                    if !path.is_dir() && !self.ignored(path)? {
                        break entry;
                    }
                }
                None => return Ok(None),
            }
        };

        BitIndexEntry::try_from(BitPath::intern(direntry.path())).map(Some)
    }
}

pub trait BitEntryIterator = BitIterator<BitIndexEntry>;

pub trait BitIterator<T> = FallibleIterator<Item = T, Error = BitGenericError>;

impl<'r> BitRepo<'r> {
    pub fn worktree_iter(self) -> BitResult<impl BitEntryIterator + 'r> {
        trace!("worktree_iter()");
        WorktreeIter::new(self)
    }

    pub fn tree_entry_iter(self, tree: &Tree) -> BitResult<impl BitEntryIterator + 'r> {
        trace!("tree_entry_iter()");
        Ok(TreeEntryIter::new(self, tree))
    }

    pub fn head_iter(self) -> BitResult<impl BitEntryIterator + 'r> {
        trace!("head_iter()");
        let tree = self.head_tree()?;
        self.tree_entry_iter(&tree)
    }
}

trait BitIteratorExt: BitEntryIterator {}

impl<I: BitEntryIterator> BitIteratorExt for I {
}

#[cfg(test)]
mod tests;
