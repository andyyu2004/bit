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
use sha1::{Digest, Sha1};
pub use tag::*;
pub use tree::*;

use crate::error::{BitGenericError, BitResult};
use crate::hash::{self, SHA1Hash};
use crate::io::BufReadExt;
use crate::repo::{BitRepo, BitRepoWeakRef};
use crate::serialize::{DeserializeSized, Serialize};
use num_enum::TryFromPrimitive;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::Metadata;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;
use std::sync::Arc;

#[derive(PartialEq, Clone)]
pub struct BitPackObjRaw {
    pub obj_type: BitObjType,
    pub bytes: Vec<u8>,
}

impl BitPackObjRaw {
    pub fn oid(&self) -> Oid {
        let mut hasher = Sha1::new();
        hasher.update(format!("{} {}\0", self.obj_type, self.bytes.len()));
        hasher.update(&self.bytes);
        SHA1Hash::new(hasher.finalize().into())
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

pub struct BitRawObj {
    cached: BitObjCached,
    stream: Box<dyn BufRead + Send>,
}

impl BitRawObj {
    pub fn from_stream(oid: Oid, mut stream: Box<dyn BufRead + Send>) -> BitResult<Self> {
        let BitObjHeader { obj_type, size } = read_obj_header(&mut stream)?;
        Ok(BitRawObj::new(oid, obj_type, size, Box::new(stream)))
    }

    pub fn new(oid: Oid, obj_type: BitObjType, size: u64, stream: Box<dyn BufRead + Send>) -> Self {
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

impl Display for BitObjKind {
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
        if f.alternate() { write!(f, "{n:o}") } else { write!(f, "{n:06o}") }
    }
}

impl Debug for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl FileMode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn is_link(self) -> bool {
        matches!(self, FileMode::LINK)
    }

    pub fn is_gitlink(self) -> bool {
        matches!(self, FileMode::GITLINK)
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
        Self::try_from(u).unwrap_or_else(|_| panic!("invalid filemode `{u:06o}`"))
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

#[derive(PartialEq, Debug, Clone, BitObject)]
pub enum BitObjKind {
    Blob(Arc<Blob>),
    Commit(Arc<Commit>),
    Tree(Arc<Tree>),
    Tag(Arc<Tag>),
}

impl BitObjKind {
    pub fn into_tree(self) -> Arc<Tree> {
        match self {
            Self::Tree(tree) => tree,
            // panicking instead of erroring as this should be called only with certainty
            _ => panic!("expected tree, found `{}`", self.obj_ty()),
        }
    }

    /// Returns `true` if the bit_obj_kind is [`Commit`].
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Commit(..))
    }
}

impl BitObjKind {
    pub fn obj_type(&self) -> BitObjType {
        match self {
            BitObjKind::Blob(_) => BitObjType::Blob,
            BitObjKind::Commit(_) => BitObjType::Commit,
            BitObjKind::Tree(_) => BitObjType::Tree,
            BitObjKind::Tag(_) => BitObjType::Tag,
        }
    }

    pub fn try_into_commit(self) -> BitResult<Arc<Commit>> {
        match self {
            Self::Commit(commit) => Ok(commit),
            _ => Err(anyhow!("expected commit found `{}`", self.obj_type())),
        }
    }

    pub fn into_commit(self) -> Arc<Commit> {
        match self {
            Self::Commit(commit) => commit,
            _ => panic!("expected commit"),
        }
    }

    pub fn into_blob(self) -> Arc<Blob> {
        match self {
            BitObjKind::Blob(blob) => blob,
            _ => panic!("expected blob"),
        }
    }

    pub fn is_tree(self) -> bool {
        matches!(self, Self::Tree(..))
    }

    pub fn is_treeish(self) -> bool {
        matches!(self, Self::Tree(..) | Self::Commit(..))
    }

    pub(crate) fn new(
        owner: BitRepoWeakRef,
        cached: BitObjCached,
        reader: impl BufRead,
    ) -> BitResult<Self> {
        match cached.obj_type {
            BitObjType::Commit => Commit::new(owner, cached, reader).map(Self::Commit),
            BitObjType::Tree => Tree::new(owner, cached, reader).map(Self::Tree),
            BitObjType::Blob => Blob::new(owner, cached, reader).map(Self::Blob),
            BitObjType::Tag => Tag::new(owner, cached, reader).map(Self::Tag),
        }
    }

    pub(crate) fn from_slice(
        owner: BitRepoWeakRef,
        cached: BitObjCached,
        slice: &[u8],
    ) -> BitResult<Self> {
        Self::new(owner, cached, BufReader::new(slice))
    }

    pub(crate) fn from_raw(owner: BitRepoWeakRef, raw: BitRawObj) -> BitResult<Self> {
        Self::new(owner, raw.cached, raw.stream)
    }
}

impl Serialize for BitObjKind {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
            BitObjKind::Tag(tag) => tag.serialize(writer),
        }
    }
}

pub trait WritableObject: Serialize + Send + Sync {
    fn obj_ty(&self) -> BitObjType;

    /// serialize objects with the header of `<type> <size>\0`
    fn serialize_with_headers(&self) -> BitResult<Vec<u8>> {
        let mut bytes = vec![];
        self.serialize(&mut bytes)?;
        let mut buf = vec![];
        write!(buf, "{} {}\0", self.obj_ty(), bytes.len())?;
        buf.extend_from_slice(&bytes);
        Ok(buf)
    }

    fn hash_and_serialize(&self) -> BitResult<(Oid, Vec<u8>)> {
        let bytes = self.serialize_with_headers()?;
        let oid = hash::hash_bytes(&bytes);
        Ok((oid, bytes))
    }

    fn hash(&self) -> BitResult<Oid> {
        self.serialize_with_headers().map(hash::hash_bytes)
    }
}

// the `Display` format!("{}") impl should pretty print
// the alternate `Display` format!("{:#}") should
// print user facing content that may not be pretty
// example is `bit cat-object tree <hash>` which just tries to print raw bytes
// often they will just be the same
// implmentors of BitObj must never be mutated otherwise their `Oid` will be wrong
pub trait BitObject {
    fn owner(&self) -> BitRepo;
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

pub(crate) trait ImmutableBitObject {
    type Mutable: DeserializeSized;

    fn new(
        owner: BitRepoWeakRef,
        cached: BitObjCached,
        reader: impl BufRead,
    ) -> BitResult<Arc<Self>>
    where
        Self: Sized,
    {
        Ok(Arc::new(Self::from_mutable(
            owner,
            cached,
            Self::Mutable::deserialize_sized(reader, cached.size)?,
        )))
    }

    fn from_mutable(owner: BitRepoWeakRef, cached: BitObjCached, inner: Self::Mutable) -> Self;
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
        write!(f, "{s}")
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
