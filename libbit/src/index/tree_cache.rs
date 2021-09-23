use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt, WriteExt};
use crate::obj::{BitObject, FileMode, Oid, Tree, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
#[cfg(test)]
use indexmap::IndexMap;
use std::io::{BufRead, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct BitTreeCache {
    /// relative path to parent
    pub path: BitPath,
    /// Oid of the corresponding tree object
    pub tree_oid: Oid,
    // -1 means invalid
    // the number of entries in the index that is covered by the tree this entry represents
    // (i.e. the number of "files" under this tree)
    pub entry_count: isize,
    // Map from path component to the tree_cache representing that directory,
    // the map's key should be the same as the child tree_cache's `path`
    // This datastructure preserves insertion order *provided there are no removals*
    // Don't think that the order of this actually matters, but it is useful for testing that deserialize and serialization are inverses
    // so we are using an "ordered" map in debug
    #[cfg(test)]
    pub children: IndexMap<BitPath, BitTreeCache>,
    #[cfg(not(test))]
    pub children: rustc_hash::FxHashMap<BitPath, BitTreeCache>,
}

impl Default for BitTreeCache {
    fn default() -> Self {
        Self {
            path: BitPath::EMPTY,
            tree_oid: Oid::UNKNOWN,
            entry_count: -1,
            children: Default::default(),
        }
    }
}

impl BitTreeCache {
    pub fn find_valid_child(&self, path: BitPath) -> Option<&Self> {
        match self.find_child(path) {
            Some(child) if child.entry_count > 0 => Some(child),
            _ => None,
        }
    }

    pub fn find_child(&self, path: BitPath) -> Option<&Self> {
        self.find_child_internal(path.components())
    }

    fn find_child_internal(&self, components: impl Iterator<Item = BitPath>) -> Option<&Self> {
        let mut child = Some(self);
        for component in components {
            child = child?.children.get(&component);
        }
        child
    }

    pub fn find_child_mut(&mut self, path: BitPath) -> Option<&mut Self> {
        self.find_child_mut_internal(path.components())
    }

    fn find_child_mut_internal(
        &mut self,
        components: impl Iterator<Item = BitPath>,
    ) -> Option<&mut Self> {
        let mut child = Some(self);
        for component in components {
            child = child?.children.get_mut(&component);
        }
        child
    }

    pub fn invalidate_path(&mut self, path: BitPath) {
        self.invalidate_path_internal(path);
    }

    pub fn invalidate_path_internal(&mut self, path: BitPath) -> Option<()> {
        self.invalidate();
        let mut child = self;
        for component in path.components() {
            child = child.children.get_mut(&component)?;
            child.invalidate();
        }
        Some(())
    }

    pub(super) fn invalidate(&mut self) {
        self.entry_count = -1;
        // the following is not strictly necessary but makes errors more obvious
        self.tree_oid = Oid::UNKNOWN;
    }

    pub fn is_valid(&self) -> bool {
        self.entry_count >= 0
    }

    pub fn is_fully_valid(&self) -> bool {
        if !self.is_valid() {
            false
        } else {
            self.children.values().all(|child| child.is_fully_valid())
        }
    }

    // Update the tree_cache to match `treeish`
    // This should only be called on the root tree_cache
    pub fn update<'rcx>(
        &mut self,
        repo: BitRepo<'rcx>,
        treeish: impl Treeish<'rcx>,
    ) -> BitResult<()> {
        let tree = treeish.treeish(repo)?;
        assert_eq!(self.path, BitPath::EMPTY);
        // we know the path of the root tree_cache is already correct as it's always just BitPath::EMPTY
        self.update_internal(repo, tree)
    }

    /// *NOTE* this method will not modify the tree_cache's path field, and so ensure the path is updated correctly
    fn update_internal<'rcx>(&mut self, repo: BitRepo<'rcx>, tree: &Tree<'rcx>) -> BitResult<()> {
        self.tree_oid = tree.oid();
        // reset the `entry_count` and count again from zero
        self.entry_count = 0;

        // Create a new set of children and steal existing children from the old cache_tree where possible.
        // This is an easy (and efficient?) way to remove deleted entries from the cache
        #[cfg(test)]
        let mut new_children = IndexMap::default();
        #[cfg(not(test))]
        let mut new_children = rustc_hash::FxHashMap::default();

        for entry in &tree.entries {
            match entry.mode {
                FileMode::REG | FileMode::EXEC | FileMode::LINK => self.entry_count += 1,
                FileMode::TREE => match self.children.get_mut(&entry.path) {
                    Some(child) => {
                        // if subtree changed or is invalid, recursively update, otherwise it is good as is
                        if !child.is_valid() || child.tree_oid != entry.oid {
                            let subtree = repo.read_obj_tree(entry.oid)?;
                            // we know the child's path is correct as we just looked it up in the map by path
                            child.update_internal(repo, &subtree)?;
                        }
                        debug_assert_eq!(
                            child,
                            &mut Self::read_tree_internal(
                                repo,
                                repo.read_obj_tree(entry.oid)?,
                                entry.path
                            )?,
                            "child was not updated in-place correctly, it should match a fresh read"
                        );
                        new_children.insert(entry.path, std::mem::take(child));
                    }
                    // new tree added
                    None => {
                        let subtree = repo.read_obj_tree(entry.oid)?;
                        let child = Self::read_tree_internal(repo, &subtree, entry.path)?;
                        new_children.insert(entry.path, child);
                    }
                },
                FileMode::GITLINK => todo!(),
            }
        }

        // add all the subtree counts to `entry_count`
        self.entry_count += new_children.values().map(|child| child.entry_count).sum::<isize>();
        self.children = new_children;

        Ok(())
    }

    pub fn read_tree<'rcx>(repo: BitRepo<'rcx>, treeish: impl Treeish<'rcx>) -> BitResult<Self> {
        let tree = treeish.treeish(repo)?;
        Self::read_tree_internal(repo, tree, BitPath::EMPTY)
    }

    fn read_tree_internal(repo: BitRepo<'_>, tree: &Tree<'_>, path: BitPath) -> BitResult<Self> {
        let mut cache_tree = Self {
            tree_oid: tree.oid(),
            entry_count: 0,
            path,
            #[cfg(test)]
            children: IndexMap::with_capacity(tree.entries.len() / 8),
            #[cfg(not(test))]
            children: rustc_hash::FxHashMap::default(),
        };

        for entry in &tree.entries {
            match entry.mode {
                FileMode::REG | FileMode::LINK | FileMode::EXEC => cache_tree.entry_count += 1,
                FileMode::TREE => {
                    let subtree = repo.read_obj_tree(entry.oid)?;
                    let child = Self::read_tree_internal(repo, subtree, entry.path)?;
                    cache_tree.entry_count += child.entry_count;
                    cache_tree.children.insert(entry.path, child);
                }
                FileMode::GITLINK => todo!(),
            }
        }

        Ok(cache_tree)
    }
}

impl Serialize for BitTreeCache {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        writer.write_null_terminated_path(self.path)?;
        writer.write_ascii_num(self.entry_count, 0x20)?;
        writer.write_ascii_num(self.children.len(), 0x0a)?;
        // only write oid when entry_count is valid
        if self.entry_count >= 0 {
            writer.write_oid(self.tree_oid)?;
        }

        for child in self.children.values() {
            child.serialize(writer)?;
        }

        Ok(())
    }
}

impl Deserialize for BitTreeCache {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let tree_cache = Self::deserialize_inner(&mut reader)?;
        assert!(reader.is_at_eof()?);
        Ok(tree_cache)
    }
}

impl BitTreeCache {
    fn deserialize_inner(reader: &mut impl BufRead) -> BitResult<Self> {
        let path = reader.read_null_terminated_path()?;
        let entry_count = reader.read_ascii_num(0x20)? as isize;
        let children_count = reader.read_ascii_num(0x0a)? as usize;

        // oid only exists when entry_count is valid
        let oid = if entry_count >= 0 { reader.read_oid()? } else { Oid::UNKNOWN };

        let children = (0..children_count)
            .map(|_| Self::deserialize_inner(reader))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|tree_cache| (tree_cache.path, tree_cache))
            .collect();
        Ok(Self { path, entry_count, children, tree_oid: oid })
    }
}
