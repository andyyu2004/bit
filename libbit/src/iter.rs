use crate::error::{BitGenericError, BitResult};
use crate::index::{BitIndexEntry};
use crate::obj::{FileMode, Tree, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use ignore::{Walk, WalkBuilder};
use std::convert::TryFrom;
use std::path::Path;

impl BitRepo {
    pub fn head_tree_iter(&self) -> BitResult<TreeIter<'_>> {
        let tree = self.head_tree()?;
        Ok(self.tree_iter(&tree))
    }

    pub fn tree_iter(&self, tree: &Tree) -> TreeIter<'_> {
        TreeIter::new(self, tree)
    }
}

#[derive(Debug)]
pub struct TreeIter<'r> {
    repo: &'r BitRepo,
    // tuple of basepath (the current path up to but not including the path of the entry) and the entry itself
    entry_stack: Vec<(BitPath, TreeEntry)>,
}

impl<'r> TreeIter<'r> {
    pub fn new(repo: &'r BitRepo, tree: &Tree) -> Self {
        Self {
            repo,
            entry_stack: tree
                .entries
                .iter()
                .cloned()
                .rev()
                .map(|entry| (BitPath::empty(), entry))
                .collect(),
        }
    }
}

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait TreeIterator: BitIterator<TreeEntry> {
    fn over(&mut self) -> BitResult<Option<TreeEntry>>;

    // seems difficult to provide a peek method just via an adaptor
    // unclear how to implement peek in terms of `over` and `next`
    // in particular, if `peek` uses `next`, then all the subdirectories would already
    // be added to the stack and its awkward to implement `over` after `peek`
    // similar problems arise with implementing `peek` using `over`
    // probably better to just let the implementor deal with it
    // especially as the implementation is probably trivial
    fn peek(&self) -> BitResult<Option<TreeEntry>>;
}

impl<'r> FallibleIterator for TreeIter<'r> {
    type Error = BitGenericError;
    type Item = TreeEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    FileMode::DIR => {
                        let tree = self.repo.read_obj(entry.hash)?.into_tree()?;
                        let path = base.join(entry.path);
                        debug!("TreeIter::next: read directory `{:?}` `{}`", path, entry.hash);
                        self.entry_stack
                            .extend(tree.entries.into_iter().rev().map(|entry| (path, entry)))
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

impl<'r> TreeIterator for TreeIter<'r> {
    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        loop {
            // TODO can dry out this code (with above) if it turns out to be what we want
            match self.entry_stack.pop() {
                Some((base, mut entry)) => match entry.mode {
                    // step over for trees returns the entry but does not recurse into it
                    FileMode::DIR | FileMode::REG | FileMode::LINK | FileMode::EXEC => {
                        debug!("HeadIter::over: entry: {:?}", entry);
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

    fn peek(&self) -> BitResult<Option<TreeEntry>> {
        Ok(self.entry_stack.last().map(|x| x.1))
    }
}

#[derive(Debug)]
struct HeadIter<'r> {
    tree_iter: TreeIter<'r>,
}

impl<'r> HeadIter<'r> {
    pub fn new(repo: &'r BitRepo, root: &Tree) -> Self {
        Self { tree_iter: TreeIter::new(repo, root) }
    }
}

impl<'r> FallibleIterator for HeadIter<'r> {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.tree_iter.next()?.map(BitIndexEntry::from))
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
    repo: &'r BitRepo,
    walk: Walk,
}

impl<'r> WorktreeIter<'r> {
    pub fn new(repo: &'r BitRepo) -> BitResult<Self> {
        Ok(Self {
            repo,
            walk: WalkBuilder::new(repo.workdir)
                .sort_by_file_path(|a, b| {
                    let mut a = BitPath::intern(a);
                    // see TreeEntry::cmp comments
                    if a.is_dir() {
                        a = a.join("=");
                    }
                    let mut b = BitPath::intern(b);
                    if b.is_dir() {
                        b = b.join("=");
                    }
                    a.cmp(&b)
                })
                .hidden(false)
                .build(),
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

impl BitRepo {
    pub fn worktree_iter(&self) -> BitResult<impl BitEntryIterator + '_> {
        trace!("worktree_iter()");
        WorktreeIter::new(self)
    }

    pub fn head_iter(&self) -> BitResult<impl BitEntryIterator + '_> {
        trace!("head_iter()");
        let tree = self.head_tree()?;
        Ok(HeadIter::new(self, &tree))
    }
}

trait BitIteratorExt: BitEntryIterator {}

impl<I: BitEntryIterator> BitIteratorExt for I {
}

#[cfg(test)]
mod tests;
