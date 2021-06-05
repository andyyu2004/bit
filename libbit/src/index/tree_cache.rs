use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt, WriteExt};
use crate::obj::{BitObj, BitObjKind, Oid, Tree};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct BitTreeCache {
    pub path: BitPath,
    // -1 means invalid
    // the number of entries in the index that is covered by the tree this entry represents
    pub entry_count: isize,
    pub children: Vec<BitTreeCache>,
    pub oid: Oid,
}

impl Default for BitTreeCache {
    fn default() -> Self {
        Self { path: BitPath::EMPTY, oid: Oid::UNKNOWN, entry_count: -1, children: vec![] }
    }
}

impl BitTreeCache {
    pub fn read_tree(repo: BitRepo<'_>, tree: &Tree) -> BitResult<Self> {
        let mut cache_tree = Self::default();
        cache_tree.oid = tree.oid();
        cache_tree.entry_count = 0;

        // alloacate a conservative amount of space assuming all entries are trees
        cache_tree.children = Vec::with_capacity(tree.entries.len());

        for entry in &tree.entries {
            match repo.read_obj(entry.oid)? {
                BitObjKind::Blob(..) => cache_tree.entry_count += 1,
                BitObjKind::Tree(subtree) => {
                    let child = Self::read_tree(repo, &subtree)?;
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
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let tree_cache = Self::deserialize_inner(reader)?;
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
