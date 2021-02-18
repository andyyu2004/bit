use crate::{BitRepo, BitResult};
use flate2::read::ZlibDecoder;
use sha2::{Digest, Sha256};
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::str::FromStr;

#[derive(PartialEq, Debug)]
pub struct Commit {}

#[derive(PartialEq, Debug)]
pub struct Blob {
    pub bytes: Vec<u8>,
}

#[derive(PartialEq, Debug)]
pub enum BitObjKind {
    Blob(Blob),
    Commit(Commit),
}

impl BitObj for Blob {
    fn serialize(&self) -> &[u8] {
        &self.bytes
    }

    fn obj_ty(&self) -> BitObjTag {
        BitObjTag::Blob
    }

    fn new(_tag: BitObjTag, bytes: &[u8]) -> Self {
        Self { bytes: bytes.to_vec() }
    }
}

impl BitObj for BitObjKind {
    fn serialize(&self) -> &[u8] {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(),
            BitObjKind::Commit(_) => todo!(),
        }
    }

    fn obj_ty(&self) -> BitObjTag {
        match self {
            BitObjKind::Blob(blob) => blob.obj_ty(),
            BitObjKind::Commit(..) => BitObjTag::Commit,
        }
    }

    fn new(tag: BitObjTag, bytes: &[u8]) -> Self {
        match tag {
            BitObjTag::Commit => todo!(),
            BitObjTag::Tree => todo!(),
            BitObjTag::Tag => todo!(),
            BitObjTag::Blob => Self::Blob(Blob::new(tag, bytes)),
        }
    }
}

pub trait BitObj {
    fn new(tag: BitObjTag, bytes: &[u8]) -> Self;
    fn serialize(&self) -> &[u8];
    fn obj_ty(&self) -> BitObjTag;
}

pub enum BitObjTag {
    Commit,
    Tree,
    Tag,
    Blob,
}

impl Display for BitObjTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            BitObjTag::Commit => "commit",
            BitObjTag::Tree => "tree",
            BitObjTag::Tag => "tag",
            BitObjTag::Blob => "blob",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for BitObjTag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "commit" => Ok(BitObjTag::Commit),
            "tree" => Ok(BitObjTag::Tree),
            "tag" => Ok(BitObjTag::Tag),
            "blob" => Ok(BitObjTag::Blob),
            _ => Err(format!("unknown bit object type `{}`", s)),
        }
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

pub fn hash_obj(obj: &impl BitObj) -> BitResult<String> {
    let bytes = serialize_obj_with_headers(obj)?;
    Ok(hash_bytes(&bytes))
}

pub fn serialize_obj_with_headers(obj: &impl BitObj) -> BitResult<Vec<u8>> {
    let mut buf = vec![];
    buf.write(obj.obj_ty().to_string().as_bytes())?;
    buf.write(&[0x20])?;
    let bytes = obj.serialize();
    buf.write(bytes.len().to_string().as_bytes())?;
    buf.write(&[0x00])?;
    buf.write(bytes)?;
    Ok(buf)
}

/// format: <type> 0x20 <size> 0x00 <content>
pub fn read_obj<R: Read>(read: R) -> BitResult<BitObjKind> {
    let mut reader = BufReader::new(read);
    let mut buf = vec![];

    let i = reader.read_until(0x20, &mut buf)?;
    let obj_ty = BitObjTag::from_str(std::str::from_utf8(&buf[..i - 1]).unwrap()).unwrap();

    let j = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[i..i + j - 1]).unwrap().parse::<usize>().unwrap();
    let len = reader.read_to_end(&mut buf)?;
    debug_assert_eq!(len, size);
    let contents = &buf[i + j..];
    assert_eq!(contents.len(), size);

    Ok(match obj_ty {
        BitObjTag::Commit => todo!(),
        BitObjTag::Tree => todo!(),
        BitObjTag::Tag => todo!(),
        BitObjTag::Blob => BitObjKind::Blob(Blob { bytes: contents.to_vec() }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut buf = vec![];
        let bit_obj = BitObjKind::Blob(Blob { bytes: b"hello".to_vec() });
        write_obj(&bit_obj, &mut buf)?;
        let parsed_bit_obj = read_obj(buf.as_slice()).unwrap();
        assert_eq!(bit_obj, parsed_bit_obj);
        Ok(())
    }
}
