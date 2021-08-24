use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt, WriteExt};
use crate::obj::{BitObjType, BitObject, Oid, Tree, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
#[cfg(test)]
use indexmap::IndexMap;
#[cfg(not(test))]
use rustc_hash::FxHashMap;
use std::io::{BufRead, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct BitTreeCache {
    /// relative path to parent
    pub path: BitPath,
    // -1 means invalid
    // the number of entries in the index that is covered by the tree this entry represents
    // (i.e. the number of "files" under this tree)
    pub entry_count: isize,
    // this datastructure preserves insertion order *provided there are no removals*
    // don't think that the order of this actually matters, but it is useful for testing that deserialize and serialization are inverses
    #[cfg(test)]
    pub children: IndexMap<BitPath, BitTreeCache>,
    #[cfg(not(test))]
    pub children: FxHashMap<BitPath, BitTreeCache>,
    pub oid: Oid,
}

impl Default for BitTreeCache {
    fn default() -> Self {
        Self {
            path: BitPath::EMPTY,
            oid: Oid::UNKNOWN,
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
        self.find_child_internal(path.components().iter().copied())
    }

    fn find_child_internal(&self, mut components: impl Iterator<Item = BitPath>) -> Option<&Self> {
        match components.next() {
            Some(next) => self.children.get(&next)?.find_child_internal(components),
            None => Some(self),
        }
    }

    pub fn find_child_mut(&mut self, path: BitPath) -> Option<&mut Self> {
        self.find_child_mut_internal(path.components().iter().copied())
    }

    fn find_child_mut_internal(
        &mut self,
        mut components: impl Iterator<Item = BitPath>,
    ) -> Option<&mut Self> {
        match components.next() {
            Some(next) => self.children.get_mut(&next)?.find_child_mut_internal(components),
            None => Some(self),
        }
    }

    pub fn invalidate_path(&mut self, path: BitPath) {
        self.entry_count = -1;
        // don't do this recursively as each path contains the full path, not just a component
        for path in path.cumulative_components() {
            if let Some(child) = self.find_child_mut(path) {
                child.entry_count = -1;
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        self.entry_count < 0
    }

    pub fn is_fully_valid(&self) -> bool {
        if self.is_valid() {
            false
        } else {
            self.children.values().all(|child| child.is_fully_valid())
        }
    }

    pub fn read_tree_cache<'rcx>(
        repo: BitRepo<'rcx>,
        treeish: impl Treeish<'rcx>,
    ) -> BitResult<Self> {
        let tree = treeish.treeish(repo)?;
        Self::read_tree_internal(repo, &tree, BitPath::EMPTY)
    }

    fn read_tree_internal(repo: BitRepo<'_>, tree: &Tree<'_>, path: BitPath) -> BitResult<Self> {
        let mut cache_tree = Self {
            oid: tree.oid(),
            entry_count: 0,
            path,
            #[cfg(test)]
            children: IndexMap::with_capacity(tree.entries.len() / 8),
            #[cfg(not(test))]
            children: FxHashMap::default(),
        };

        for entry in &tree.entries {
            match repo.read_obj_header(entry.oid)?.obj_type {
                BitObjType::Blob => cache_tree.entry_count += 1,
                BitObjType::Tree => {
                    let subtree = repo.read_obj(entry.oid)?.into_tree();
                    let child = Self::read_tree_internal(repo, &subtree, entry.path)?;
                    cache_tree.entry_count += child.entry_count;
                    cache_tree.children.insert(entry.path, child);
                }
                BitObjType::Commit | BitObjType::Tag => unreachable!(),
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
        if self.entry_count >= 0 {
            writer.write_oid(self.oid)?;
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
        Ok(Self { path, entry_count, children, oid })
    }
}
