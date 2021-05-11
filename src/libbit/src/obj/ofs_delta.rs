use crate::delta::Delta;
use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

use super::{BitObj, BitObjType};

#[derive(PartialEq, Clone, Debug)]
pub struct OfsDelta {
    pub offset: u64,
    pub delta: Delta,
}

impl Serialize for OfsDelta {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for OfsDelta {
    fn deserialize_sized(reader: &mut impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let offset = reader.read_offset()?;
        let delta = Delta::deserialize_sized(&mut reader.into_zlib_decode_stream(), delta_size)?;
        Ok(Self { offset, delta })
    }

    // we encode the offset in the first 8 bytes (network order) followed by the raw delta
    fn deserialize_sized_raw(reader: &mut impl BufRead, size: u64) -> BitResult<Vec<u8>>
    where
        Self: Sized,
    {
        let offset = reader.read_offset()?;
        let mut buf = Vec::with_capacity(8);
        buf.extend_from_slice(&offset.to_be_bytes());
        reader.into_zlib_decode_stream().take(size).read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl BitObj for OfsDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::OfsDelta
    }
}
