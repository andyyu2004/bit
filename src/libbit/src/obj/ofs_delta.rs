use crate::error::BitResult;
use crate::serialize::{Deserialize, DeserializeSized, Serialize};
use std::io::prelude::*;

use super::{BitObj, BitObjType};

#[derive(PartialEq, Clone, Debug)]
pub struct OfsDelta {
    offset: u64,
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
        todo!()
    }
}

impl BitObj for OfsDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::OfsDelta
    }
}
