mod commit;
mod obj_id;
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
    fn serialize(&self) -> &[u8] {
        &self.bytes
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Blob
    }

    fn deserialize(_bytes: &[u8]) -> Self {
        todo!()
    }
}

impl BitObj for BitObjKind {
    fn serialize(&self) -> &[u8] {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(),
            BitObjKind::Commit(_) => todo!(),
            BitObjKind::Tree(_) => todo!(),
        }
    }

    fn deserialize(_bytes: &[u8]) -> Self {
        todo!()
    }

    fn obj_ty(&self) -> BitObjType {
        match self {
            BitObjKind::Blob(blob) => blob.obj_ty(),
            BitObjKind::Commit(..) => BitObjType::Commit,
            BitObjKind::Tree(_) => todo!(),
        }
    }
}

pub trait BitObj {
    fn serialize(&self) -> &[u8];
    fn deserialize(bytes: &[u8]) -> Self;
    fn obj_ty(&self) -> BitObjType;
}

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
    let bytes = obj.serialize();
    write!(buf, "{}\0", bytes.len().to_string())?;
    buf.write_all(bytes)?;
    Ok(buf)
}

/// format: <type>0x20<size>0x00<content>
pub fn read_obj<R: Read>(read: R) -> BitResult<BitObjKind> {
    let mut reader = BufReader::new(read);
    let mut buf = vec![];

    let i = reader.read_until(0x20, &mut buf)?;
    let obj_ty = std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap();

    let j = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[i..i + j - 1]).unwrap().parse::<usize>().unwrap();
    let len = reader.read_to_end(&mut buf)?;
    debug_assert_eq!(len, size);
    let contents = &buf[i + j..];
    assert_eq!(contents.len(), size);

    Ok(match obj_ty {
        BitObjType::Commit => BitObjKind::Commit(Commit::parse(contents)?),
        BitObjType::Tree => BitObjKind::Tree(Tree::parse(contents)?),
        BitObjType::Blob => BitObjKind::Blob(Blob { bytes: contents.to_vec() }),
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
