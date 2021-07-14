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
use crate::util;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::iter::FromIterator;
use std::ops::Deref;

pub trait Treeish<'rcx> {
    fn treeish(self, repo: BitRepo<'rcx>) -> BitResult<Tree<'rcx>>;
}

impl<'rcx> Treeish<'rcx> for Tree<'rcx> {
    fn treeish(self, _repo: BitRepo<'rcx>) -> BitResult<Self> {
        Ok(self)
    }
}

impl<'rcx> Treeish<'rcx> for Oid {
    fn treeish(self, repo: BitRepo<'rcx>) -> BitResult<Tree<'rcx>> {
        repo.read_obj(self)?.treeish(repo)
    }
}

impl<'rcx> Treeish<'rcx> for BitObjKind<'rcx> {
    fn treeish(self, repo: BitRepo<'rcx>) -> BitResult<Tree<'rcx>> {
        match self {
            BitObjKind::Commit(commit) => commit.peel(repo),
            BitObjKind::Tree(tree) => Ok(*tree),
            BitObjKind::Tag(_) => todo!(),
            BitObjKind::Blob(..) => bug!("blob is not treeish"),
        }
    }
}

impl Display for Tree<'_> {
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
pub struct Tree<'rcx> {
    owner: BitRepo<'rcx>,
    cached: BitObjCached,
    inner: MutableTree,
}

impl Tree<'_> {
    pub const EMPTY_SIZE: u64 = 0;

    #[cfg(test)]
    pub fn into_mutable(self) -> MutableTree {
        self.inner
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct MutableTree {
    pub entries: BTreeSet<TreeEntry>,
}

impl Deref for Tree<'_> {
    type Target = MutableTree;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
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

impl Default for MutableTree {
    fn default() -> Self {
        Self::new(Default::default())
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

impl BitObject for Tree<'_> {
    fn obj_cached(&self) -> &BitObjCached {
        &self.cached
    }
}

impl<'rcx> ImmutableBitObject<'rcx> for Tree<'rcx> {
    type Mutable = MutableTree;

    fn from_mutable(owner: BitRepo<'rcx>, cached: BitObjCached, inner: Self::Mutable) -> Self {
        Self { owner, cached, inner }
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
        BitPath::path_cmp(self.sort_path().as_ref(), other.sort_path().as_ref())
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
        // fairly disgusting way of deserializing a path..
        let path = util::path_from_bytes(&buf[i..i + j - 1]);

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
