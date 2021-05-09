use crate::delta::Delta;
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::io::ReadExt;
use crate::serialize::{Deserialize, DeserializeSized, Serialize};
use std::io::prelude::*;

use super::{BitObj, BitObjType};

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
    fn deserialize_sized(reader: &mut dyn BufRead, delta_size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let base_oid = reader.read_bit_hash()?;
        let delta = Delta::deserialize_sized(reader, delta_size)?;
        Ok(Self { base_oid, delta })
    }
}

impl BitObj for RefDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::RefDelta
    }
}
