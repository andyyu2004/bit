mod blob;
mod commit;
mod obj_id;
mod ofs_delta;
mod ref_delta;
mod tag;
mod tree;

pub use blob::Blob;
pub use commit::*;
pub use obj_id::*;
pub use tag::Tag;
pub use tree::{Tree, TreeEntry, Treeish};

use self::ofs_delta::OfsDelta;
use self::ref_delta::RefDelta;
use crate::error::{BitGenericError, BitResult};
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{Deserialize, DeserializeSized, Serialize};
use num_enum::TryFromPrimitive;
use std::cell::Cell;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::Metadata;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::prelude::PermissionsExt;
use std::str::FromStr;

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
    // TODO rename to tree
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

#[derive(PartialEq, Debug)]
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
    pub fn deserialize_as(
        contents: &mut impl BufRead,
        obj_ty: BitObjType,
        size: u64,
    ) -> BitResult<Self> {
        match obj_ty {
            BitObjType::Commit => Commit::deserialize_sized(contents, size).map(Self::Commit),
            BitObjType::Tree => Tree::deserialize_sized(contents, size).map(Self::Tree),
            BitObjType::Blob => Blob::deserialize_sized(contents, size).map(Self::Blob),
            BitObjType::Tag => Tag::deserialize(contents).map(Self::Tag),
            BitObjType::OfsDelta => OfsDelta::deserialize_sized(contents, size).map(Self::OfsDelta),
            BitObjType::RefDelta => RefDelta::deserialize_sized(contents, size).map(Self::RefDelta),
        }
    }

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

// NOTE! this includes reading the object header
impl Deserialize for BitObjKind {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self> {
        self::read_obj(reader)
    }
}

// very boring impl which just delegates to the inner type
impl BitObj for BitObjKind {
    fn obj_shared(&self) -> &BitObjShared {
        match self {
            BitObjKind::Blob(blob) => blob.obj_shared(),
            BitObjKind::Commit(commit) => commit.obj_shared(),
            BitObjKind::Tree(tree) => tree.obj_shared(),
            BitObjKind::Tag(tag) => tag.obj_shared(),
            BitObjKind::OfsDelta(ofs_delta) => ofs_delta.obj_shared(),
            BitObjKind::RefDelta(ref_delta) => ref_delta.obj_shared(),
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct BitObjShared {
    oid: Cell<Oid>,
    ty: BitObjType,
}

impl BitObjShared {
    pub fn with_oid(ty: BitObjType, oid: Oid) -> Self {
        Self { ty, oid: Cell::new(oid) }
    }

    pub fn new(ty: BitObjType) -> Self {
        Self { ty, oid: Cell::new(Oid::UNKNOWN) }
    }
}

// the `Display` format!("{}") impl should pretty print
// the alternate `Display` format!("{:#}") should
// print user facing content that may not be pretty
// example is `bit cat-object tree <hash>` which just tries to print raw bytes
// often they will just be the same
// implmentors of BitObj must never be mutated otherwise their `Oid` will be wrong
pub trait BitObj: Serialize + DeserializeSized + Debug {
    fn obj_shared(&self) -> &BitObjShared;

    fn obj_ty(&self) -> BitObjType {
        self.obj_shared().ty
    }

    fn oid(&self) -> Oid {
        let oid_cell = &self.obj_shared().oid;
        let mut oid = oid_cell.get();
        // should this ever occur assuming every calls set_oid correctly?
        // i.e. is this a bug
        if oid.is_unknown() {
            oid = crate::hash::hash_obj(self).expect("shouldn't really fail");
            oid_cell.set(oid);
        }
        oid
    }

    // not a fan of this api, very prone to just not setting it and then requiring an unnessary hash of the object,
    //  and we have to set it from places that are a bit weird?
    fn set_oid(&self, oid: Oid) {
        self.obj_shared().oid.set(oid)
    }

    /// serialize objects append on the header of `type len`
    fn serialize_with_headers(&self) -> BitResult<Vec<u8>> {
        let mut buf = vec![];
        write!(buf, "{} ", self.obj_ty())?;
        let mut bytes = vec![];
        self.serialize(&mut bytes)?;
        write!(buf, "{}\0", bytes.len())?;
        buf.write_all(&bytes)?;
        Ok(buf)
    }
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

pub(crate) fn read_obj_header(reader: &mut impl BufRead) -> BitResult<BitObjHeader> {
    let obj_type = reader.read_ascii_str(0x20)?;
    let size = reader.read_ascii_num(0x00)? as u64;
    Ok(BitObjHeader { obj_type, size })
}

#[cfg(test)]
pub(crate) fn read_obj_unbuffered(reader: impl std::io::Read) -> BitResult<BitObjKind> {
    read_obj(&mut BufReader::new(reader))
}

/// format: <type>0x20<size>0x00<content>
pub(crate) fn read_obj(reader: &mut impl BufRead) -> BitResult<BitObjKind> {
    let header = read_obj_header(reader)?;
    let buf = reader.read_to_vec()?;
    let contents = buf.as_slice();
    assert_eq!(contents.len() as u64, header.size);
    let contents = &mut BufReader::new(contents);
    let size = header.size as u64;

    Ok(match header.obj_type {
        BitObjType::Commit => BitObjKind::Commit(Commit::deserialize_sized(contents, size)?),
        BitObjType::Tree => BitObjKind::Tree(Tree::deserialize_sized(contents, size)?),
        BitObjType::Blob => BitObjKind::Blob(Blob::deserialize_sized(contents, size)?),
        BitObjType::Tag => todo!(),
        BitObjType::OfsDelta => todo!(),
        BitObjType::RefDelta => todo!(),
    })
}

#[cfg(test)]
mod commit_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
