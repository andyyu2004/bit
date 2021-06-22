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

use self::ofs_delta::OfsDelta;
use self::ref_delta::RefDelta;
use crate::delta::Delta;
use crate::error::{BitGenericError, BitResult};
use crate::io::BufReadExt;
use crate::serialize::{DeserializeSized, Serialize};
use num_enum::TryFromPrimitive;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::Metadata;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;

#[derive(PartialEq)]
pub struct BitObjRaw {
    pub obj_type: BitObjType,
    pub bytes: Vec<u8>,
}

impl BitObjRaw {
    pub fn expand_with_delta(&self, delta: &Delta) -> BitResult<Self> {
        trace!("BitObjRaw::expand_with_delta(..)");
        //? is it guaranteed that the (expanded) base of a delta is of the same type?
        let &Self { obj_type, ref bytes } = self;
        Ok(Self { obj_type, bytes: delta.expand(bytes)? })
    }
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

#[derive(Debug)]
pub struct BitOdbRawObj<S: BufRead> {
    cached: BitObjCached,
    stream: S,
}

impl<S: BufRead> BitOdbRawObj<S> {
    pub fn new(oid: Oid, obj_type: BitObjType, size: u64, stream: S) -> Self {
        Self { stream, cached: BitObjCached { oid, size, obj_type } }
    }
}

impl Debug for BitObjRaw {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.obj_type)
    }
}

impl Display for BitObjKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // we can't write the following as `write!(f, "{}", x)
        // as we would lose the flags on the formatter
        match self {
            BitObjKind::Blob(blob) => Display::fmt(&blob, f),
            BitObjKind::Commit(commit) => Display::fmt(&commit, f),
            BitObjKind::Tree(tree) => Display::fmt(&tree, f),
            BitObjKind::Tag(_) => todo!(),
            BitObjKind::OfsDelta(_) => todo!(),
            BitObjKind::RefDelta(_) => todo!(),
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

    pub fn is_file(self) -> bool {
        matches!(self, FileMode::EXEC | FileMode::REG | FileMode::LINK)
    }

    pub fn is_tree(self) -> bool {
        matches!(self, FileMode::TREE)
    }

    pub fn new(u: u32) -> Self {
        Self::try_from(u).unwrap_or_else(|_| panic!("invalid filemode `{}`", u))
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
            _ => unreachable!("invalid filemode for obj {}", self),
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
pub enum BitObjKind {
    Blob(Blob),
    Commit(Commit),
    Tree(Tree),
    Tag(Tag),
    OfsDelta(OfsDelta),
    RefDelta(RefDelta),
}

impl Treeish for BitObjKind {
    fn into_tree(self) -> BitResult<Tree> {
        match self {
            Self::Tree(tree) => Ok(tree),
            // panicking instead of erroring as this should be called only with certainty
            _ => panic!("expected tree, found `{}`", self.obj_ty()),
        }
    }
}

impl BitObjKind {
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

    pub fn into_commit(self) -> Commit {
        match self {
            Self::Commit(commit) => commit,
            _ => panic!("expected commit"),
        }
    }

    pub fn into_blob(self) -> Blob {
        match self {
            BitObjKind::Blob(blob) => blob,
            _ => panic!("expected blob"),
        }
    }

    pub fn is_tree(&self) -> bool {
        matches!(self, Self::Tree(..))
    }

    pub fn new(cached: BitObjCached, reader: impl BufRead) -> BitResult<Self> {
        match cached.obj_type {
            BitObjType::Commit => Commit::new(cached, reader).map(Self::Commit),
            BitObjType::Tree => Tree::new(cached, reader).map(Self::Tree),
            BitObjType::Blob => Blob::new(cached, reader).map(Self::Blob),
            BitObjType::Tag => Tag::new(cached, reader).map(Self::Tag),
            // try and eliminate these two cases when not in a packfile context
            BitObjType::OfsDelta => todo!(),
            BitObjType::RefDelta => todo!(),
        }
    }

    pub fn from_slice(cached: BitObjCached, slice: &[u8]) -> BitResult<Self> {
        Self::new(cached, BufReader::new(slice))
    }

    pub fn from_odb_obj(odb_obj: BitOdbRawObj<impl BufRead>) -> BitResult<Self> {
        Self::new(odb_obj.cached, odb_obj.stream)
    }
}

impl Serialize for BitObjKind {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
            BitObjKind::Tag(tag) => tag.serialize(writer),
            BitObjKind::OfsDelta(ofs_delta) => ofs_delta.serialize(writer),
            BitObjKind::RefDelta(ref_delta) => ref_delta.serialize(writer),
        }
    }
}

pub trait WritableObject: Serialize {
    fn obj_ty(&self) -> BitObjType;

    /// serialize objects append on the header of `type len`
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
pub trait BitObject {
    fn obj_cached(&self) -> &BitObjCached;

    fn obj_ty(&self) -> BitObjType {
        self.obj_cached().obj_type
    }

    fn oid(&self) -> Oid {
        self.obj_cached().oid
    }
}

pub trait ImmutableBitObject {
    type Mutable: DeserializeSized;

    fn new(cached: BitObjCached, reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(Self::from_mutable(cached, Self::Mutable::deserialize_sized(reader, cached.size)?))
    }

    fn from_mutable(cached: BitObjCached, inner: Self::Mutable) -> Self;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, FromPrimitive, ToPrimitive)]
pub enum BitObjType {
    Commit   = 1,
    Tree     = 2,
    Blob     = 3,
    Tag      = 4,
    OfsDelta = 6,
    RefDelta = 7,
}

impl BitObjType {
    pub fn is_delta(self) -> bool {
        match self {
            BitObjType::Commit | BitObjType::Tree | BitObjType::Blob | BitObjType::Tag => false,
            BitObjType::OfsDelta | BitObjType::RefDelta => true,
        }
    }
}

impl Display for BitObjType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            BitObjType::Commit => "commit",
            BitObjType::Tree => "tree",
            BitObjType::Tag => "tag",
            BitObjType::Blob => "blob",
            BitObjType::OfsDelta => "ofs-delta",
            BitObjType::RefDelta => "ref-delta",
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
