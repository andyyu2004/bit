use super::{BitObjCached, BitObject};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct OfsDelta {
    obj: BitObjCached,
    pub offset: u64,
    pub delta: Delta,
}

impl Serialize for OfsDelta {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for OfsDelta {
    fn deserialize_sized(mut reader: impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let _offset = reader.read_offset()?;
        let _delta = Delta::deserialize_sized(&mut reader.as_zlib_decode_stream(), delta_size)?;
        todo!()
        // let obj = BitObjCached::new(BitObjType::OfsDelta);
        // Ok(Self { obj, offset, delta })
    }
}
