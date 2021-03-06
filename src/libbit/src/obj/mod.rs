mod commit;
mod obj_id;
mod refs;
mod tree;

pub use commit::Commit;
pub use obj_id::BitObjId;
pub use tree::{Tree, TreeEntry};

use crate::error::BitResult;
use std::fmt::{self, Display, Formatter};
use std::io::{BufRead, BufReader, Read, Write};
use std::str::FromStr;

impl Display for BitObjKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitObjKind::Blob(blob) => write!(f, "{}", blob),
            BitObjKind::Commit(commit) => write!(f, "{}", commit),
            BitObjKind::Tree(tree) => write!(f, "{}", tree),
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct Blob {
    pub bytes: Vec<u8>,
}

impl Display for Blob {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(&self.bytes) {
            Ok(utf8) => write!(f, "{}", utf8),
            Err(..) => write!(f, "<binary>"),
        }
    }
}

impl Blob {
    pub fn from_reader<R: Read>(mut reader: R) -> BitResult<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        Ok(Self::new(bytes))
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[derive(PartialEq, Debug)]
pub enum BitObjKind {
    Blob(Blob),
    Commit(Commit),
    Tree(Tree),
}

impl BitObjKind {
    pub fn as_blob(self) -> Blob {
        match self {
            BitObjKind::Blob(blob) => blob,
            _ => panic!("expected blob"),
        }
    }
}

impl BitObj for Blob {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        writer.write_all(&self.bytes)?;
        Ok(())
    }

    fn deserialize<R: Read>(mut reader: R) -> BitResult<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        Ok(Self { bytes })
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Blob
    }
}

// very boring impl which just delegates to the inner type
impl BitObj for BitObjKind {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(writer),
            BitObjKind::Commit(commit) => commit.serialize(writer),
            BitObjKind::Tree(tree) => tree.serialize(writer),
        }
    }

    fn deserialize<R: Read>(reader: R) -> BitResult<Self> {
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

pub trait BitObj: Sized {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()>;
    fn deserialize<R: Read>(reader: R) -> BitResult<Self>;
    fn obj_ty(&self) -> BitObjType;
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
    let mut buf = vec![];
    write!(buf, "{} ", obj.obj_ty())?;
    let mut bytes = vec![];
    obj.serialize(&mut bytes)?;
    write!(buf, "{}\0", bytes.len().to_string())?;
    buf.write_all(&bytes)?;
    Ok(buf)
}

pub fn read_obj_type_buffered<R: BufRead>(reader: &mut R) -> BitResult<BitObjType> {
    let mut buf = vec![];
    let i = reader.read_until(0x20, &mut buf)?;
    Ok(std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap())
}

pub fn read_obj_type<R: Read>(reader: R) -> BitResult<BitObjKind> {
    read_obj_buffered(&mut BufReader::new(reader))
}

pub fn read_obj<R: Read>(read: R) -> BitResult<BitObjKind> {
    read_obj_buffered(&mut BufReader::new(read))
}

/// format: <type>0x20<size>0x00<content>
pub fn read_obj_buffered<R: BufRead>(reader: &mut R) -> BitResult<BitObjKind> {
    let mut buf = vec![];

    let obj_ty = read_obj_type_buffered(reader)?;

    let j = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[..j - 1]).unwrap().parse::<usize>().unwrap();
    let len = reader.read_to_end(&mut buf)?;
    debug_assert_eq!(len, size);
    let contents = &buf[j..];
    assert_eq!(contents.len(), size);

    Ok(match obj_ty {
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
