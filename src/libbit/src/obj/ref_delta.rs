use super::{BitObj, BitObjType};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct RefDelta {
    pub base_oid: BitHash,
    pub delta: Delta,
}

impl Serialize for RefDelta {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for RefDelta {
    fn deserialize_sized(reader: &mut impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let base_oid = reader.read_oid()?;
        let delta = Delta::deserialize_sized(&mut reader.into_zlib_decode_stream(), delta_size)?;
        Ok(Self { base_oid, delta })
    }

    // we encode the offset in the first 20 bytes (network order) followed by the raw delta
    fn deserialize_sized_raw(reader: &mut impl BufRead, size: u64) -> BitResult<Vec<u8>>
    where
        Self: Sized,
    {
        let oid = reader.read_oid()?;
        let mut buf = Vec::with_capacity(20);
        buf.extend_from_slice(oid.as_bytes());
        reader.into_zlib_decode_stream().take(size).read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl BitObj for RefDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::RefDelta
    }
}
