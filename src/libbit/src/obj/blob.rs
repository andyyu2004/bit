use super::{BitObj, BitObjType};
use crate::error::BitResult;
use crate::serialize::Serialize;
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
}

impl Serialize for Blob {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

impl BitObj for Blob {
    fn deserialize<R: BufRead>(reader: &mut R) -> BitResult<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        Ok(Self { bytes })
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Blob
    }
}
