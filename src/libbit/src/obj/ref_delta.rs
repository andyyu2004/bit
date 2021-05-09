use super::{BitObj, BitObjType};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct RefDelta {
    base_oid: BitHash,
    delta: Delta,
}

impl Serialize for RefDelta {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for RefDelta {
    fn deserialize_sized(mut reader: &mut dyn BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let base_oid = reader.read_bit_hash()?;
        let delta = Delta::deserialize_sized(&mut reader.into_zlib_decode_stream(), delta_size)?;
        Ok(Self { base_oid, delta })
    }
}

impl BitObj for RefDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::RefDelta
    }
}
