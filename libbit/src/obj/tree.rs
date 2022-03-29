use super::{BitObjCached, BitObjKind, FileMode, ImmutableBitObject, WritableObject};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::io::BufReadExt;
use crate::iter::BitEntry;
use crate::obj::{BitObjType, BitObject, Oid};
use crate::path::BitPath;
use crate::peel::Peel;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, DeserializeSized, Serialize};
use crate::tls;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::iter::FromIterator;
use std::sync::Arc;

/// Represents entities that have a straightforward way of being converted/dereferenced into a tree
/// This includes commits and oids (and also trivially tree's themselves),
/// but probaby should not extend so far as to include refs/revisions
pub trait Treeish: Sized {
    fn treeish_oid(&self, repo: &BitRepo) -> BitResult<Oid>;

    // override this default implementation if a more efficient one is available
    fn treeish(self, repo: &BitRepo) -> BitResult<Arc<Tree>> {
        let tree_oid = self.treeish_oid(repo)?;
        repo.read_obj_tree(tree_oid)
    }
}

impl Treeish for Arc<Tree> {
    fn treeish(self, _repo: &BitRepo) -> BitResult<Self> {
        Ok(self)
    }

    fn treeish_oid(&self, _repo: &BitRepo) -> BitResult<Oid> {
        Ok(self.oid())
    }
}

impl Treeish for Oid {
    fn treeish_oid(&self, repo: &BitRepo) -> BitResult<Oid> {
        repo.read_obj(*self)?.treeish_oid(repo)
    }
}

impl Treeish for BitObjKind {
    fn treeish(self, repo: &BitRepo) -> BitResult<Arc<Tree>> {
        match self {
            BitObjKind::Commit(commit) => commit.peel(repo),
            BitObjKind::Tree(tree) => Ok(tree),
            BitObjKind::Tag(..) => todo!(),
            BitObjKind::Blob(..) => bug!("blob is not treeish"),
        }
    }

    fn treeish_oid(&self, _repo: &BitRepo) -> BitResult<Oid> {
        match self {
            BitObjKind::Commit(commit) => Ok(commit.tree_oid()),
            BitObjKind::Tree(tree) => Ok(tree.oid()),
            BitObjKind::Tag(..) => todo!(),
            BitObjKind::Blob(..) => bug!("blob is not treeish"),
        }
    }
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            for entry in &self.entries {
                write!(f, "{:#}", entry)?;
            }
        } else {
            for entry in &self.entries {
                writeln!(f, "{}", entry)?;
            }
        }
        Ok(())
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{} {}\0{}", self.mode, self.path, unsafe {
                // SAFETY we'rcxe just printing this out and not using it anywhere
                std::str::from_utf8_unchecked(self.oid.as_ref())
            })
        } else {
            let obj_type = self.mode.infer_obj_type();
            debug_assert_eq!(
                obj_type,
                tls::with_repo(|repo| repo.read_obj_header(self.oid).unwrap().obj_type)
            );
            write!(f, "{} {} {}\t{}", self.mode, obj_type, self.oid, self.path)
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Tree {
    owner: BitRepo,
    cached: BitObjCached,
    // This is an exception to the other objects where immutable is not just a wrapper over mutable
    // we prefer a vec here as btreeset takes O(nlogn) to build which is unnecessarily slow
    // as the only reason that structure was used was to guarantee correct ordering after insertions/deletions.
    // We can implement deserialization more efficiently for immutable trees by just using a vector
    pub(crate) entries: Vec<TreeEntry>,
}

impl Serialize for Tree {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }
}

impl BitObject for Tree {
    fn obj_cached(&self) -> &BitObjCached {
        &self.cached
    }

    fn owner(&self) -> BitRepo {
        self.owner
    }
}

impl ImmutableBitObject for Tree {
    type Mutable = MutableTree;

    fn new(owner: BitRepo, cached: BitObjCached, reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(Self { owner, cached, entries: Self::read_entries(reader, cached.size)? })
    }

    fn from_mutable(_owner: BitRepo, _cached: BitObjCached, _inner: Self::Mutable) -> Self {
        unreachable!(
            "method unnecessary for this new design (as it's only used for a reasonable default impl for `ImmutableBitObject::new`), so this trait probably needs a rethink"
        )
    }
}
impl Tree {
    pub fn empty(repo: BitRepo) -> Arc<Self> {
        let tree = Self {
            owner: repo,
            cached: BitObjCached::new(Oid::EMPTY_TREE, BitObjType::Tree, 0),
            entries: vec![],
        };
        repo.alloc_tree(tree)
    }

    fn read_entries(r: impl BufRead, size: u64) -> BitResult<Vec<TreeEntry>>
    where
        Self: Sized,
    {
        let mut r = r.take(size);
        let mut entries = Vec::with_capacity(size as usize / 30);
        while !r.is_at_eof()? {
            let entry = TreeEntry::deserialize(&mut r)?;
            entries.push(entry);
        }

        Ok(entries)
    }

    #[cfg(test)]
    pub fn to_mutable(&self) -> MutableTree {
        MutableTree::new(self.entries.iter().copied().collect())
    }
}

#[derive(PartialEq, Debug, Clone, Default)]
pub struct MutableTree {
    pub entries: BTreeSet<TreeEntry>,
}

impl FromIterator<TreeEntry> for MutableTree {
    fn from_iter<T: IntoIterator<Item = TreeEntry>>(iter: T) -> Self {
        Self::new(iter.into_iter().collect())
    }
}

impl MutableTree {
    pub fn new(entries: BTreeSet<TreeEntry>) -> Self {
        Self { entries }
    }
}

impl Serialize for MutableTree {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }
}

impl DeserializeSized for MutableTree {
    fn deserialize_sized(r: impl BufRead, size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut r = r.take(size);

        let mut tree = Self::default();
        #[cfg(debug_assertions)]
        let mut v = vec![];

        while !r.is_at_eof()? {
            let entry = TreeEntry::deserialize(&mut r)?;
            #[cfg(debug_assertions)]
            v.push(entry);
            tree.entries.insert(entry);
        }

        // these debug assertions are checking that the btreeset ordering
        // is consistent with the order of the tree entries on disk
        // NOTE: this cfg is actually required as `debug_assert` only uses `if (cfg!(debug_assertions))`
        #[cfg(debug_assertions)]
        debug_assert_eq!(tree.entries.iter().cloned().collect::<Vec<_>>(), v);
        Ok(tree)
    }
}

impl WritableObject for MutableTree {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Tree
    }
}

#[derive(PartialEq, Debug, Clone, Eq, Copy)]
pub struct TreeEntry {
    pub mode: FileMode,
    pub path: BitPath,
    pub oid: Oid,
}

// provide explicit impl on references to avoid some unnecessary copying
impl<'a> From<&'a BitIndexEntry> for TreeEntry {
    fn from(entry: &'a BitIndexEntry) -> Self {
        Self { mode: entry.mode, path: entry.path, oid: entry.oid }
    }
}

impl From<BitIndexEntry> for TreeEntry {
    fn from(entry: BitIndexEntry) -> Self {
        let BitIndexEntry { mode, path, oid, .. } = entry;
        Self { mode, path, oid }
    }
}

impl PartialOrd for TreeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TreeEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.entry_cmp(other)
    }
}

impl BitEntry for TreeEntry {
    fn oid(&self) -> Oid {
        self.oid
    }

    fn path(&self) -> BitPath {
        self.path
    }

    fn mode(&self) -> FileMode {
        self.mode
    }
}

impl Deserialize for TreeEntry {
    fn deserialize(mut r: impl BufRead) -> BitResult<Self> {
        let mut buf = vec![];
        let i = r.read_until(0x20, &mut buf)?;
        let mode = FileMode::new(
            u32::from_str_radix(std::str::from_utf8(&buf[..i - 1]).unwrap(), 8).unwrap(),
        );

        let j = r.read_until(0x00, &mut buf)?;
        let path = BitPath::from_bytes(&buf[i..i + j - 1]);

        let mut hash_bytes = [0; 20];
        r.read_exact(&mut hash_bytes)?;
        let oid = Oid::new(hash_bytes);
        Ok(Self { mode, path, oid })
    }
}

impl Serialize for TreeEntry {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        // use alternate display impl to not pad an extra 0
        write!(writer, "{:#}", self.mode)?;
        writer.write_all(b" ")?;
        write!(writer, "{}", self.path)?;
        writer.write_all(b"\0")?;
        writer.write_all(self.oid.as_ref())?;
        Ok(())
    }
}
