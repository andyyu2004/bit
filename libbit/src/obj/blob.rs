use super::{BitObjCached, BitObjType, BitObject, ImmutableBitObject, WritableObject};
use crate::error::BitResult;
use crate::io::ReadExt;
use crate::serialize::{DeserializeSized, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::ops::Deref;

#[derive(PartialEq, Debug)]
pub struct Blob {
    cached: BitObjCached,
    inner: MutableBlob,
}

#[derive(PartialEq, Debug)]
pub struct MutableBlob {
    bytes: Vec<u8>,
}

impl Deref for Blob {
    type Target = MutableBlob;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl WritableObject for MutableBlob {
    fn obj_ty(&self) -> BitObjType {
        super::BitObjType::Blob
    }
}

impl Display for Blob {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(&self.bytes) {
            Ok(utf8) => write!(f, "{}", utf8),
            Err(..) => write!(f, "<binary>"),
        }
    }
}

impl MutableBlob {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn from_reader<R: Read>(mut reader: R) -> BitResult<Self> {
        Ok(Self::new(reader.read_to_vec()?))
    }
}

impl Serialize for MutableBlob {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

impl DeserializeSized for MutableBlob {
    fn deserialize_sized(reader: impl BufRead, size: u64) -> BitResult<Self> {
        let bytes = reader.take(size).read_to_vec()?;
        Ok(Self::new(bytes))
    }
}

impl BitObject for Blob {
    fn obj_cached(&self) -> &BitObjCached {
        &self.cached
    }
}

impl ImmutableBitObject for Blob {
    type Mutable = MutableBlob;

    fn from_mutable(cached: BitObjCached, inner: Self::Mutable) -> Self {
        Self { cached, inner }
    }
}
