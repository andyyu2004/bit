mod blob;
mod commit;
mod obj_id;
mod refs;
mod tree;

pub use blob::Blob;
pub use commit::Commit;
pub use obj_id::BitId;
pub use tree::{Tree, TreeEntry};

use crate::error::{BitGenericError, BitResult};
use crate::io_ext::ReadExt;
use crate::serialize::Serialize;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{BufRead, BufReader, Read, Write};
use std::str::FromStr;

impl Display for BitObjKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // we can't write the following as `write!(f, "{}", x)
        // as we would lose the flags on the formatter
        match self {
            BitObjKind::Blob(blob) => Display::fmt(&blob, f),
            BitObjKind::Commit(commit) => Display::fmt(&commit, f),
            BitObjKind::Tree(tree) => Display::fmt(&tree, f),
        }
    }
}

// 100644 normal
// 100755 executable
// 40000 directory
#[derive(Copy, PartialEq, Eq, Clone)]
pub struct FileMode(u32);

impl Display for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() { write!(f, "{:o}", self.0) } else { write!(f, "{:06o}", self.0) }
    }
}

impl Debug for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl FileMode {
    pub const DIR: Self = Self(Self::IFDIR);
    pub const EXEC: Self = Self(Self::IFEXEC);
    /* Directory.  */
    pub const IFDIR: u32 = 0o40000;
    /* Executable file.  */
    // this one is not defined in sysstat.h
    pub const IFEXEC: u32 = 0o100755;
    /* These bits determine file type.  */
    const IFFMT: u32 = 0o170000;
    /* Symbolic link.  */
    pub const IFLNK: u32 = 0o120000;
    /* Regular file.  */
    pub const IFREG: u32 = 0o100644;
    pub const REG: Self = Self(Self::IFREG);

    #[cfg(debug_assertions)]
    pub fn inner(self) -> u32 {
        self.0
    }

    pub fn infer_obj_type(self) -> BitObjType {
        match self {
            Self::DIR => BitObjType::Tree,
            Self::EXEC | Self::REG => BitObjType::Blob,
            _ => unreachable!("invalid filemode {}", self),
        }
    }

    pub fn is_type(self, mask: u32) -> bool {
        self.0 & Self::IFFMT == mask
    }

    pub fn is_dir(self) -> bool {
        self.is_type(Self::IFDIR)
    }

    pub fn is_link(self) -> bool {
        self.is_type(Self::IFLNK)
    }

    pub fn is_reg(self) -> bool {
        self.is_type(Self::IFREG)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl FromStr for FileMode {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mode = Self(u32::from_str_radix(s, 8)?);
        assert!(
            mode == Self::DIR || mode == Self::EXEC || mode == Self::REG,
            "invalid bit file mode `{}`",
            mode
        );
        Ok(mode)
    }
}

impl FileMode {
    pub const fn new(u: u32) -> Self {
        Self(u)
    }
}

#[derive(PartialEq, Debug)]
pub struct BitObjHeader {
    pub obj_type: BitObjType,
    pub size: usize,
}

#[derive(PartialEq, Debug)]
pub enum BitObjKind {
    Blob(Blob),
    Commit(Commit),
    Tree(Tree),
}

impl BitObjKind {
    pub fn as_tree(self) -> Tree {
        match self {
            Self::Tree(tree) => tree,
            _ => panic!("expected tree"),
        }
    }

    pub fn as_blob(self) -> Blob {
        match self {
            BitObjKind::Blob(blob) => blob,
            _ => panic!("expected blob"),
        }
    }
}

impl Serialize for BitObjKind {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
        }
    }
}

// very boring impl which just delegates to the inner type
impl BitObj for BitObjKind {
    fn deserialize<R: BufRead>(reader: &mut R) -> BitResult<Self> {
        self::read_obj(reader)
    }

    // TODO this is kinda dumb
    // try make this method unnecssary
    fn obj_ty(&self) -> BitObjType {
        match self {
            BitObjKind::Blob(blob) => blob.obj_ty(),
            BitObjKind::Commit(commit) => commit.obj_ty(),
            BitObjKind::Tree(tree) => tree.obj_ty(),
        }
    }
}

// the `Display` format!("{}") impl should pretty print
// the alternate `Display` format!("{:#}") should
// print user facing content that may not be pretty
// example is `bit cat-object tree <hash>` which just tries to print raw bytes
// often they will just be the same
pub trait BitObj: Serialize + Debug + Display {
    fn deserialize<R: BufRead>(reader: &mut R) -> BitResult<Self>
    where
        Self: Sized;

    // convenience method that will wrap a BufReader around the raw `read` instance
    fn deserialize_unbuffered<R: Read>(reader: R) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize(&mut BufReader::new(reader))
    }

    fn obj_ty(&self) -> BitObjType;

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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum BitObjType {
    Commit,
    Tree,
    Tag,
    Blob,
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
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "commit" => Ok(BitObjType::Commit),
            "tree" => Ok(BitObjType::Tree),
            "tag" => Ok(BitObjType::Tag),
            "blob" => Ok(BitObjType::Blob),
            _ => Err(format!("unknown bit object type `{}`", s)),
        }
    }
}

pub(crate) fn read_obj_header<R: BufRead>(reader: &mut R) -> BitResult<BitObjHeader> {
    let obj_type = read_obj_type(reader)?;
    let size = read_obj_size(reader)?;
    Ok(BitObjHeader { obj_type, size })
}

fn read_obj_type<R: BufRead>(reader: &mut R) -> BitResult<BitObjType> {
    let mut buf = vec![];
    let i = reader.read_until(0x20, &mut buf)?;
    Ok(std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap())
}

/// assumes <type> has been read already
fn read_obj_size<R: BufRead>(reader: &mut R) -> BitResult<usize> {
    let mut buf = vec![];
    let i = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap();
    Ok(size)
}

pub fn read_obj_unbuffered<R: Read>(reader: R) -> BitResult<BitObjKind> {
    read_obj(&mut BufReader::new(reader))
}

/// format: <type>0x20<size>0x00<content>
pub(crate) fn read_obj<R: BufRead>(reader: &mut R) -> BitResult<BitObjKind> {
    let header = read_obj_header(reader)?;
    let buf = reader.read_to_vec()?;
    let contents = buf.as_slice();
    assert_eq!(contents.len(), header.size);
    let contents = &mut BufReader::new(contents);

    Ok(match header.obj_type {
        BitObjType::Commit => BitObjKind::Commit(Commit::deserialize(contents)?),
        BitObjType::Tree => BitObjKind::Tree(Tree::deserialize(contents)?),
        BitObjType::Blob => BitObjKind::Blob(Blob::deserialize(contents)?),
        BitObjType::Tag => todo!(),
    })
}

#[cfg(test)]
mod tests;
