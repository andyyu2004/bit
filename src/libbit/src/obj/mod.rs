use crate::{BitRepo, BitResult};
use flate2::read::ZlibDecoder;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::str::FromStr;

#[derive(PartialEq, Debug)]
pub struct Commit {}

#[derive(PartialEq, Debug)]
pub struct Blob {
    bytes: Vec<u8>,
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

    fn obj_ty(&self) -> ObjType {
        ObjType::Blob
    }
}

impl BitObj for BitObjKind {
    fn serialize(&self) -> &[u8] {
        match self {
            BitObjKind::Blob(blob) => blob.serialize(),
            BitObjKind::Commit(_) => todo!(),
        }
    }

    fn obj_ty(&self) -> ObjType {
        match self {
            BitObjKind::Blob(blob) => blob.obj_ty(),
            BitObjKind::Commit(..) => ObjType::Commit,
        }
    }
}

pub trait BitObj {
    fn serialize(&self) -> &[u8];
    fn obj_ty(&self) -> ObjType;
}

pub enum ObjType {
    Commit,
    Tree,
    Tag,
    Blob,
}

impl Display for ObjType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            ObjType::Commit => "commit",
            ObjType::Tree => "tree",
            ObjType::Tag => "tag",
            ObjType::Blob => "blob",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for ObjType {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "commit" => ObjType::Commit,
            "tree" => ObjType::Tree,
            "tag" => ObjType::Tag,
            "blob" => ObjType::Blob,
            _ => panic!("unknown object type `{}`", s),
        })
    }
}

pub fn write_obj<W: Write>(obj: &impl BitObj, mut writer: W) -> BitResult<()> {
    writer.write(obj.obj_ty().to_string().as_bytes())?;
    writer.write(&[0x20])?;
    let bytes = obj.serialize();
    writer.write(bytes.len().to_string().as_bytes())?;
    writer.write(&[0x00])?;
    writer.write(bytes)?;
    Ok(())
}

pub fn read_obj<R: Read>(read: R) -> BitResult<BitObjKind> {
    let mut reader = BufReader::new(read);
    let mut buf = vec![];

    let i = reader.read_until(0x20, &mut buf)?;
    let obj_ty = ObjType::from_str(std::str::from_utf8(&buf[..i - 1]).unwrap()).unwrap();

    let j = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[i..i + j - 1]).unwrap().parse::<usize>().unwrap();
    let len = reader.read_to_end(&mut buf)?;
    debug_assert_eq!(len, size);
    let contents = &buf[i + j..];
    assert_eq!(contents.len(), size);

    Ok(match obj_ty {
        ObjType::Commit => todo!(),
        ObjType::Tree => todo!(),
        ObjType::Tag => todo!(),
        ObjType::Blob => BitObjKind::Blob(Blob { bytes: contents.to_vec() }),
    })
}

impl BitRepo {
    /// format: <type> 0x20 <size> 0x00 <content>
    pub fn read_obj_from_hash(&self, hash: &str) -> BitResult<BitObjKind> {
        let obj_path = self.relative_paths(&["objects", &hash[0..2], &hash[2..]]);
        let z = ZlibDecoder::new(File::open(obj_path)?);
        read_obj(z)
    }
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
