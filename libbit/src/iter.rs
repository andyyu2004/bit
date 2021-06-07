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

/// tree iterators allow stepping over entire trees (skipping all entries recursively)
pub trait TreeIterator: BitIterator<TreeEntry> {
    /// unstable semantics
    /// if the next entry is a tree then yield the tree entry but skip over its contents
    /// otherwise does the same as next
    /// `next` should always yield the tree entry itself
    fn over(&mut self) -> BitResult<Option<TreeEntry>>;

    // seems difficult to provide a peek method just via an adaptor
    // unclear how to implement peek in terms of `over` and `next`
    // in particular, if `peek` uses `next`, then all the subdirectories would already
    // be added to the stack and its awkward to implement `over` after `peek`
    // similar problems arise with implementing `peek` using `over`
    // probably better to just let the implementor deal with it
    // especially as the implementation is probably trivial
    fn peek(&mut self) -> BitResult<Option<TreeEntry>>;
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

impl<'r> TreeIterator for TreeIter<'r> {
    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        match self.next()? {
            Some(entry) => {
                if entry.mode == FileMode::DIR {
                    self.dir_entries.take();
                }
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn peek(&mut self) -> BitResult<Option<TreeEntry>> {
        Ok(self.entry_stack.last().map(|x| x.1))
    }
}

pub struct IndexTreeIter<'a, 'r> {
    index: &'a BitIndex<'r>,
    entry_iter: Peekable<IndexEntryIterator>,
    // pseudotrees that have been yielded
    pseudotrees: HashSet<BitPath>,
    peeked: Option<TreeEntry>,
}

impl<'a, 'r> IndexTreeIter<'a, 'r> {
    pub fn new(index: &'a BitIndex<'r>) -> Self {
        Self {
            index,
            peeked: None,
            entry_iter: index.iter().peekable(),
            pseudotrees: hashset! { BitPath::EMPTY },
        }
    }

    fn create_pseudotree(&self, path: BitPath) -> TreeEntry {
        let oid = self
            .index
            .tree_cache()
            .and_then(|cache| cache.find_valid_child(path))
            .map(|child| child.oid)
            .unwrap_or(Oid::UNKNOWN);
        TreeEntry { mode: FileMode::DIR, path, oid }
    }

    fn step_over_dir(&mut self, dir_entry: TreeEntry) -> BitResult<Option<TreeEntry>> {
        if let Some(tree_cache) =
            self.index.tree_cache().and_then(|cache| cache.find_valid_child(dir_entry.path))
        {
            // for `n` entries we want to call next `n` times which is what `nth(n-1)` will do
            // we must call `nth` on the inner iterator as that is what `entry_count` corresponds to
            self.entry_iter.nth(tree_cache.entry_count as usize - 1)?;
        } else {
            // step over this tree by stepping over each of its children
            while self.peek()?.map(|next| next.path.starts_with(dir_entry.path)).unwrap_or(false) {
                self.over()?;
            }
        }
        Ok(Some(dir_entry))
    }
}

impl<'a, 'r> FallibleIterator for IndexTreeIter<'a, 'r> {
    type Error = BitGenericError;
    type Item = TreeEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(peeked) = self.peeked.take() {
            return Ok(Some(peeked));
        }

        let next = self.entry_iter.peek()?.map(TreeEntry::from);
        match next {
            Some(entry) => {
                let dir = entry.path.parent().unwrap();
                if self.pseudotrees.insert(dir) {
                    Ok(Some(self.create_pseudotree(dir)))
                } else {
                    self.entry_iter.next()?;
                    Ok(next)
                }
            }
            None => return Ok(None),
        }
    }
}

impl<'a, 'r> TreeIterator for IndexTreeIter<'a, 'r> {
    fn peek(&mut self) -> BitResult<Option<TreeEntry>> {
        if let Some(peeked) = self.peeked {
            Ok(Some(peeked))
        } else {
            self.peeked = self.next()?;
            Ok(self.peeked)
        }
    }

    fn over(&mut self) -> BitResult<Option<TreeEntry>> {
        match self.next()? {
            Some(entry) => match entry.mode {
                FileMode::DIR => self.step_over_dir(entry),
                FileMode::REG | FileMode::EXEC | FileMode::LINK => Ok(Some(entry)),
                FileMode::GITLINK => todo!(),
                _ => unreachable!(),
            },
            None => return Ok(None),
        }
    }
}

#[derive(Debug)]
struct TreeEntryIter<'r> {
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
