use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt, WriteExt};
use crate::obj::{BitObjKind, BitObject, Oid, Tree};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct BitTreeCache {
    pub path: BitPath,
    // -1 means invalid
    // the number of entries in the index that is covered by the tree this entry represents
    // (i.e. the number of "files" under this tree)
    pub entry_count: isize,
    pub children: Vec<BitTreeCache>,
    pub oid: Oid,
}

impl Default for BitTreeCache {
    fn default() -> Self {
        Self { path: BitPath::EMPTY, oid: Oid::UNKNOWN, entry_count: -1, children: vec![] }
    }
}

macro_rules! find_child_base_case {
    ($self:expr, $path:ident) => {
        let $path = $path.as_ref();
        debug_assert!($path.is_relative());
        if $self.path.as_path() == $path {
            return Some($self);
        }
    };
}

impl BitTreeCache {
    pub fn find_valid_child(&self, path: impl AsRef<Path>) -> Option<&BitTreeCache> {
        match self.find_child(path) {
            Some(child) if child.entry_count > 0 => Some(child),
            _ => None,
        }
    }

    pub fn find_child(&self, path: impl AsRef<Path>) -> Option<&BitTreeCache> {
        find_child_base_case!(self, path);
        self.children.iter().find_map(|child| child.find_child(path))
    }

    pub fn find_child_mut(&mut self, path: impl AsRef<Path>) -> Option<&mut BitTreeCache> {
        find_child_base_case!(self, path);
        self.children.iter_mut().find_map(|child| child.find_child_mut(path))
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
            self.children.iter().all(|child| child.is_fully_valid())
        }
    }

    pub fn read_tree_cache(repo: BitRepo<'_>, tree: &Tree) -> BitResult<Self> {
        Self::read_tree_internal(repo, tree, BitPath::EMPTY)
    }

    fn read_tree_internal(repo: BitRepo<'_>, tree: &Tree, path: BitPath) -> BitResult<Self> {
        let mut cache_tree = Self::default();
        cache_tree.oid = tree.oid();
        cache_tree.entry_count = 0;
        cache_tree.path = path;

        // alloacate a conservative amount of space assuming all entries are trees
        cache_tree.children = Vec::with_capacity(tree.entries.len());

        for entry in &tree.entries {
            match repo.read_obj(entry.oid)? {
                BitObjKind::Blob(..) => cache_tree.entry_count += 1,
                BitObjKind::Tree(subtree) => {
                    let child = Self::read_tree_internal(repo, &subtree, path.join(entry.path))?;
                    cache_tree.entry_count += child.entry_count;
                    cache_tree.children.push(child);
                }
                BitObjKind::Commit(..)
                | BitObjKind::Tag(..)
                | BitObjKind::OfsDelta(..)
                | BitObjKind::RefDelta(..) => unreachable!(),
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

        for child in &self.children {
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
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { path, entry_count, children, oid })
    }
}
