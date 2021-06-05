use super::{BitObj, BitObjShared, BitObjType};
use crate::error::BitResult;
use crate::io::ReadExt;
use crate::serialize::{DeserializeSized, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

#[derive(PartialEq, Debug)]
pub struct Blob {
    obj: BitObjShared,
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
        Ok(Self::new(reader.read_to_vec()?))
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, obj: BitObjShared::new(BitObjType::Blob) }
    }
}

impl Serialize for Blob {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

impl DeserializeSized for Blob {
    fn deserialize_sized(reader: &mut impl BufRead, size: u64) -> BitResult<Self> {
        let bytes = reader.take(size).read_to_vec()?;
        Ok(Self::new(bytes))
    }
}

impl BitObj for Blob {
    fn obj_shared(&self) -> &BitObjShared {
        &self.obj
    }
}
