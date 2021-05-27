use super::{BitObj, BitObjType, Oid};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct RefDelta {
    pub base_oid: Oid,
    pub delta: Delta,
}

impl Serialize for RefDelta {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for RefDelta {
    fn deserialize_sized(reader: &mut impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let base_oid = reader.read_oid()?;
        let delta = Delta::deserialize_sized(&mut reader.as_zlib_decode_stream(), delta_size)?;
        Ok(Self { base_oid, delta })
    }
}

impl BitObj for RefDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::RefDelta
    }
}
