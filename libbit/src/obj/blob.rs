use super::{BitObjCached, BitObjType, BitObject, ImmutableBitObject, WritableObject};
use crate::error::BitResult;
use crate::io::ReadExt;
use crate::repo::BitRepo;
use crate::serialize::{DeserializeSized, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::ops::Deref;

#[derive(PartialEq, Debug)]
pub struct Blob<'rcx> {
    owner: BitRepo<'rcx>,
    cached: BitObjCached,
    inner: MutableBlob,
}

#[derive(PartialEq, Debug)]
pub struct MutableBlob {
    bytes: Vec<u8>,
}

impl Deref for Blob<'_> {
    type Target = MutableBlob;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl WritableObject for MutableBlob {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Blob
    }
}

impl Blob<'_> {
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.bytes
    }
}

impl Display for Blob<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(&self.bytes) {
            Ok(utf8) => write!(f, "{}", utf8.trim_end()),
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

impl<'rcx> BitObject<'rcx> for Blob<'rcx> {
    fn obj_cached(&self) -> &BitObjCached {
        &self.cached
    }

    fn owner(&self) -> BitRepo<'rcx> {
        self.owner
    }
}

impl<'rcx> ImmutableBitObject<'rcx> for Blob<'rcx> {
    type Mutable = MutableBlob;

    fn from_mutable(owner: BitRepo<'rcx>, cached: BitObjCached, inner: Self::Mutable) -> Self {
        Self { owner, cached, inner }
    }
}
