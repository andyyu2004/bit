mod blob;
mod commit;
mod obj_id;
mod ofs_delta;
mod ref_delta;
mod tag;
mod tree;

pub use blob::*;
pub use commit::*;
pub use obj_id::*;
pub use tag::*;
pub use tree::*;

use crate::error::{BitGenericError, BitResult};
use crate::io::BufReadExt;
use crate::repo::BitRepo;
use crate::serialize::{DeserializeSized, Serialize};
use num_enum::TryFromPrimitive;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::Metadata;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;

#[derive(PartialEq)]
pub struct BitPackObjRaw {
    pub obj_type: BitObjType,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitObjCached {
    oid: Oid,
    // cached from object header
    obj_type: BitObjType,
    // cached from object header
    size: u64,
}

impl BitObjCached {
    pub fn new(oid: Oid, obj_type: BitObjType, size: u64) -> Self {
        Self { oid, obj_type, size }
    }

    pub fn oid(&self) -> Oid {
        self.oid
    }

    pub fn obj_type(&self) -> BitObjType {
        self.obj_type
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

pub struct BitRawObj {
    cached: BitObjCached,
    stream: Box<dyn BufRead>,
}

impl BitRawObj {
    pub fn new(oid: Oid, obj_type: BitObjType, size: u64, stream: Box<dyn BufRead>) -> Self {
        Self { stream, cached: BitObjCached { oid, size, obj_type } }
    }

    pub fn from_raw_pack_obj(oid: Oid, raw: BitPackObjRaw) -> Self {
        let cached = BitObjCached::new(oid, raw.obj_type, raw.bytes.len() as u64);
        Self { cached, stream: Box::new(Cursor::new(raw.bytes)) }
    }
}

impl Debug for BitPackObjRaw {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.obj_type)
    }
}

impl Display for BitObjKind<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // we can't write the following as `write!(f, "{}", x)
        // as we would lose the flags on the formatter
        // actually why would we lose the flags? we have to pass in f to write! anyway?
        match self {
            BitObjKind::Blob(blob) => Display::fmt(&blob, f),
            BitObjKind::Commit(commit) => Display::fmt(&commit, f),
            BitObjKind::Tree(tree) => Display::fmt(&tree, f),
            BitObjKind::Tag(_) => todo!(),
        }
    }
}

#[derive(Copy, PartialEq, Eq, Clone, TryFromPrimitive, PartialOrd, Ord)]
#[repr(u32)]
// the ordering of variants is significant here as it implements `Ord`
// we want `TREE` to be ordered after the "file" variants
// don't know about `GITLINK` yet
pub enum FileMode {
    REG     = 0o100644,
    EXEC    = 0o100755,
    LINK    = 0o120000,
    TREE    = 0o40000,
    GITLINK = 0o160000,
}

impl Display for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let n = self.as_u32();
        if f.alternate() { write!(f, "{:o}", n) } else { write!(f, "{:06o}", n) }
    }
}

impl Debug for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl FileMode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn is_link(self) -> bool {
        matches!(self, FileMode::LINK)
    }

    pub fn is_file(self) -> bool {
        matches!(self, FileMode::EXEC | FileMode::REG)
    }

    pub fn is_blob(self) -> bool {
        matches!(self, FileMode::EXEC | FileMode::REG | FileMode::LINK)
    }

    pub fn is_tree(self) -> bool {
        matches!(self, FileMode::TREE)
    }

    pub fn new(u: u32) -> Self {
        Self::try_from(u).unwrap_or_else(|_| panic!("invalid filemode `{:06o}`", u))
    }

    pub fn from_metadata(metadata: &Metadata) -> Self {
        if metadata.file_type().is_symlink() {
            Self::LINK
        } else if metadata.is_dir() {
            Self::TREE
        } else {
            let permissions = metadata.permissions();
            let is_executable = permissions.mode() & 0o111;
            if is_executable != 0 { Self::EXEC } else { Self::REG }
        }
    }

    pub fn infer_obj_type(self) -> BitObjType {
        match self {
            Self::TREE => BitObjType::Tree,
            Self::EXEC | Self::REG | Self::LINK => BitObjType::Blob,
            _ => unreachable!("invalid filemode for obj `{}`", self),
        }
    }
}

impl FromStr for FileMode {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(u32::from_str_radix(s, 8)?))
    }
}

#[derive(PartialEq, Debug)]
pub struct BitObjHeader {
    pub obj_type: BitObjType,
    pub size: u64,
}

#[derive(PartialEq, Debug, BitObject)]
pub enum BitObjKind<'rcx> {
    Blob(Box<Blob<'rcx>>),
    Commit(Box<Commit<'rcx>>),
    Tree(Box<Tree<'rcx>>),
    Tag(Box<Tag<'rcx>>),
}

impl<'rcx> BitObjKind<'rcx> {
    // TODO bitobjkind doesn't impl `Treeish` as the signature isn't ideal as we don't actually need the repo here
    // to consider later
    pub fn into_tree(self) -> BitResult<Tree<'rcx>> {
        match self {
            Self::Tree(tree) => Ok(*tree),
            // panicking instead of erroring as this should be called only with certainty
            _ => panic!("expected tree, found `{}`", self.obj_ty()),
        }
    }

    /// Returns `true` if the bit_obj_kind is [`Commit`].
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Commit(..))
    }
}

impl<'rcx> BitObjKind<'rcx> {
    /// deserialize into a `BitObjKind` given an object type and "size"
    /// (this is similar to [crate::serialize::DeserializeSized])
    // pub fn deserialize_as(
    //     contents: impl BufRead,
    //     obj_ty: BitObjType,
    //     size: u64,
    // ) -> BitResult<Self> {
    //     match obj_ty {
    //         BitObjType::Commit => Commit::deserialize_sized(contents, size).map(Self::Commit),
    //         BitObjType::Tree => Tree::deserialize_sized(contents, size).map(Self::Tree),
    //         BitObjType::Blob => Blob::deserialize_sized(contents, size).map(Self::Blob),
    //         BitObjType::Tag => Tag::deserialize(contents).map(Self::Tag),
    //         BitObjType::OfsDelta => OfsDelta::deserialize_sized(contents, size).map(Self::OfsDelta),
    //         BitObjType::RefDelta => RefDelta::deserialize_sized(contents, size).map(Self::RefDelta),
    //     }
    // }

    pub fn obj_type(&self) -> BitObjType {
        match self {
            BitObjKind::Blob(_) => BitObjType::Blob,
            BitObjKind::Commit(_) => BitObjType::Commit,
            BitObjKind::Tree(_) => BitObjType::Tree,
            BitObjKind::Tag(_) => BitObjType::Tag,
        }
    }

    pub fn try_into_commit(self) -> BitResult<Commit<'rcx>> {
        match self {
            Self::Commit(commit) => Ok(*commit),
            _ => Err(anyhow!("expected commit found `{}`", self.obj_type())),
        }
    }

    pub fn into_commit(self) -> Commit<'rcx> {
        match self {
            Self::Commit(commit) => *commit,
            _ => panic!("expected commit"),
        }
    }

    pub fn into_blob(self) -> Blob<'rcx> {
        match self {
            BitObjKind::Blob(blob) => *blob,
            _ => panic!("expected blob"),
        }
    }

    pub fn is_tree(&self) -> bool {
        matches!(self, Self::Tree(..))
    }

    pub fn is_treeish(&self) -> bool {
        matches!(self, Self::Tree(..) | Self::Commit(..))
    }

    pub fn new(
        owner: BitRepo<'rcx>,
        cached: BitObjCached,
        reader: impl BufRead,
    ) -> BitResult<Self> {
        match cached.obj_type {
            BitObjType::Commit =>
                Commit::new(owner, cached, reader).map(Box::new).map(Self::Commit),
            BitObjType::Tree => Tree::new(owner, cached, reader).map(Box::new).map(Self::Tree),
            BitObjType::Blob => Blob::new(owner, cached, reader).map(Box::new).map(Self::Blob),
            BitObjType::Tag => Tag::new(owner, cached, reader).map(Box::new).map(Self::Tag),
        }
    }

    pub fn from_slice(owner: BitRepo<'rcx>, cached: BitObjCached, slice: &[u8]) -> BitResult<Self> {
        Self::new(owner, cached, BufReader::new(slice))
    }

    pub fn from_raw(owner: BitRepo<'rcx>, raw: BitRawObj) -> BitResult<Self> {
        Self::new(owner, raw.cached, raw.stream)
    }
}

impl Serialize for BitObjKind<'_> {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
            BitObjKind::Tag(tag) => tag.serialize(writer),
        }
    }
}

pub trait WritableObject: Serialize {
    fn obj_ty(&self) -> BitObjType;

    /// serialize objects with the header of `<type> <size>\0`
    fn serialize_with_headers(&self) -> BitResult<Vec<u8>> {
        let mut buf = vec![];
        write!(buf, "{} ", self.obj_ty())?;
        let mut bytes = vec![];
        self.serialize(&mut bytes)?;
        write!(buf, "{}\0", bytes.len())?;
        buf.extend_from_slice(&bytes);
        Ok(buf)
    }
}

// the `Display` format!("{}") impl should pretty print
// the alternate `Display` format!("{:#}") should
// print user facing content that may not be pretty
// example is `bit cat-object tree <hash>` which just tries to print raw bytes
// often they will just be the same
// implmentors of BitObj must never be mutated otherwise their `Oid` will be wrong
pub trait BitObject<'rcx> {
    fn owner(&self) -> BitRepo<'rcx>;
    fn obj_cached(&self) -> &BitObjCached;

    #[inline]
    fn obj_ty(&self) -> BitObjType {
        self.obj_cached().obj_type
    }

    #[inline]
    fn oid(&self) -> Oid {
        self.obj_cached().oid
    }
}

pub trait ImmutableBitObject<'rcx> {
    type Mutable: DeserializeSized;

    fn new(owner: BitRepo<'rcx>, cached: BitObjCached, reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(Self::from_mutable(
            owner,
            cached,
            Self::Mutable::deserialize_sized(reader, cached.size)?,
        ))
    }

    fn from_mutable(owner: BitRepo<'rcx>, cached: BitObjCached, inner: Self::Mutable) -> Self;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, FromPrimitive, ToPrimitive)]
pub enum BitObjType {
    Commit = 1,
    Tree   = 2,
    Blob   = 3,
    Tag    = 4,
}

impl Display for BitObjType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            BitObjType::Commit => "commit",
            BitObjType::Tree => "tree",
            BitObjType::Tag => "tag",
            BitObjType::Blob => "blob",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for BitObjType {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "commit" => Ok(BitObjType::Commit),
            "tree" => Ok(BitObjType::Tree),
            "tag" => Ok(BitObjType::Tag),
            "blob" => Ok(BitObjType::Blob),
            _ => bail!("unknown bit object type `{}`", s),
        }
    }
}

pub(crate) fn read_obj_header(mut reader: impl BufRead) -> BitResult<BitObjHeader> {
    let obj_type = reader.read_ascii_str(0x20)?;
    let size = reader.read_ascii_num(0x00)? as u64;
    Ok(BitObjHeader { obj_type, size })
}

#[cfg(test)]
mod commit_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
