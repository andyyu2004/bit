use crate::error::BitResult;
use crate::serialize::{Deserialize, Serialize};
use std::io::prelude::*;

use super::{BitObj, BitObjType};

#[derive(PartialEq, Clone, Debug)]
pub struct RefDelta {
    base_oid: u64,
    delta: (),
}

impl Serialize for RefDelta {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl Deserialize for RefDelta {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl BitObj for RefDelta {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::RefDelta
    }
}
