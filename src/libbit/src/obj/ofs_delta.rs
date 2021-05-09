use crate::error::BitResult;
use crate::serialize::{Deserialize, Serialize};
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

impl Deserialize for OfsDelta {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
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
