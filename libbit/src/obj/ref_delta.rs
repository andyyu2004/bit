use super::{BitObjCached, BitObject, Oid};
use crate::delta::Delta;
use crate::error::BitResult;
use crate::io::{BufReadExt, ReadExt};
use crate::serialize::{DeserializeSized, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct RefDelta {
    obj: BitObjCached,
    pub base_oid: Oid,
    pub delta: Delta,
}

impl Serialize for RefDelta {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl DeserializeSized for RefDelta {
    fn deserialize_sized(mut reader: impl BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let _base_oid = reader.read_oid()?;
        let _delta = Delta::deserialize_sized(&mut reader.as_zlib_decode_stream(), delta_size)?;
        todo!()
        // let obj = BitObjCached::new(BitObjType::RefDelta);
        // Ok(Self { obj, base_oid, delta })
    }
}
