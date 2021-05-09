use super::{BitObj, BitObjType};
use crate::error::BitResult;
use crate::serialize::{Deserialize, Serialize};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct Tag {}

impl Serialize for Tag {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl Deserialize for Tag {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl BitObj for Tag {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Tag
    }
}
