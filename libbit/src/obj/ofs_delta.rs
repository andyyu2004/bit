use super::{BitObj, BitObjType};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct OfsDelta {
    pub offset: u64,
    pub delta: Delta,
}

impl Serialize for OfsDelta {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for OfsDelta {
    fn deserialize_sized(reader: &mut impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let offset = reader.read_offset()?;
        let delta = Delta::deserialize_sized(&mut reader.as_zlib_decode_stream(), delta_size)?;
        Ok(Self { offset, delta })
    }
}

impl BitObj for OfsDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::OfsDelta
    }
}
