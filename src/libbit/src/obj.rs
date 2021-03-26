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
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
        }
    }
}

// very boring impl which just delegates to the inner type
impl BitObj for BitObjKind {
    fn deserialize<R: Read>(reader: R) -> BitResult<Self> {
        self::read_obj(reader)
    }

    fn deserialize_buffered<R: BufRead>(reader: &mut R) -> BitResult<Self> {
        self::read_obj_buffered(reader)
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
pub trait BitObj: Serialize + Sized + Debug + Display {
    fn deserialize_buffered<R: BufRead>(reader: &mut R) -> BitResult<Self>;
    fn deserialize<R: Read>(reader: R) -> BitResult<Self> {
        Self::deserialize_buffered(&mut BufReader::new(reader))
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

pub fn serialize_obj_with_headers(obj: &impl BitObj) -> BitResult<Vec<u8>> {
    obj.serialize_with_headers()
}

pub fn read_obj_header_buffered<R: BufRead>(reader: &mut R) -> BitResult<BitObjHeader> {
    let obj_type = read_obj_type_buffered(reader)?;
    let size = read_obj_size_buffered(reader)?;
    Ok(BitObjHeader { obj_type, size })
}

pub fn read_obj_header<R: Read>(reader: R) -> BitResult<BitObjHeader> {
    read_obj_header_buffered(&mut BufReader::new(reader))
}

pub fn read_obj_type_buffered<R: BufRead>(reader: &mut R) -> BitResult<BitObjType> {
    let mut buf = vec![];
    let i = reader.read_until(0x20, &mut buf)?;
    Ok(std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap())
}

pub fn read_obj_type<R: Read>(reader: R) -> BitResult<BitObjType> {
    read_obj_type_buffered(&mut BufReader::new(reader))
}

pub fn read_obj<R: Read>(read: R) -> BitResult<BitObjKind> {
    read_obj_buffered(&mut BufReader::new(read))
}

/// assumes <type> has been read already
pub fn read_obj_size_buffered<R: BufRead>(reader: &mut R) -> BitResult<usize> {
    let mut buf = vec![];
    let i = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap();
    Ok(size)
}

/// format: <type>0x20<size>0x00<content>
pub fn read_obj_buffered<R: BufRead>(reader: &mut R) -> BitResult<BitObjKind> {
    let header = read_obj_header_buffered(reader)?;
    let buf = reader.read_to_vec()?;
    let contents = buf.as_slice();
    debug_assert_eq!(contents.len(), header.size);
    assert_eq!(contents.len(), header.size);

    Ok(match header.obj_type {
        BitObjType::Commit => BitObjKind::Commit(Commit::deserialize(contents)?),
        BitObjType::Tree => BitObjKind::Tree(Tree::deserialize(contents)?),
        BitObjType::Blob => BitObjKind::Blob(Blob::deserialize(contents)?),
        BitObjType::Tag => todo!(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[test]
    fn valid_obj_read() {
        let mut bytes = vec![];
        bytes.extend(b"blob ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xywz");
        read_obj(bytes.as_slice()).unwrap();
    }

    #[test]
    #[should_panic]
    fn invalid_obj_read_wrong_size() {
        let mut bytes = vec![];
        bytes.extend(b"blob ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xyw");

        let _ = read_obj(bytes.as_slice());
    }

    #[test]
    #[should_panic]
    fn invalid_obj_read_unknown_obj_ty() {
        let mut bytes = vec![];
        bytes.extend(b"weirdobjty ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xywz");

        let _ = read_obj(bytes.as_slice());
    }

    #[test]
    fn write_read_blob_obj() -> BitResult<()> {
        let bit_obj = BitObjKind::Blob(Blob { bytes: b"hello".to_vec() });
        let bytes = serialize_obj_with_headers(&bit_obj)?;
        let parsed_bit_obj = read_obj(bytes.as_slice()).unwrap();
        assert_eq!(bit_obj, parsed_bit_obj);
        Ok(())
    }

    #[quickcheck]
    fn read_write_blob_obj_preserves_bytes(bytes: Vec<u8>) -> BitResult<()> {
        let bit_obj = BitObjKind::Blob(Blob { bytes });
        let serialized = serialize_obj_with_headers(&bit_obj)?;
        let parsed_bit_obj = read_obj(serialized.as_slice()).unwrap();
        assert_eq!(bit_obj, parsed_bit_obj);
        Ok(())
    }
}
