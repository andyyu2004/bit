use super::{BitObj, BitObjType};
use crate::error::BitResult;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

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

    fn deserialize_shared<R: Read>(reader: &mut R) -> BitResult<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        Ok(Self { bytes })
    }
}

impl BitObj for Blob {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        writer.write_all(&self.bytes)?;
        Ok(())
    }

    fn deserialize<R: Read>(mut reader: R) -> BitResult<Self> {
        Self::deserialize_shared(&mut reader)
    }

    fn deserialize_buffered<R: BufRead>(reader: &mut R) -> BitResult<Self> {
        Self::deserialize_shared(reader)
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Blob
    }
}

