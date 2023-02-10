use super::{BitObjCached, BitObject, ImmutableBitObject};
use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::io::prelude::*;
use std::ops::Deref;

#[derive(PartialEq, Clone, Debug)]
pub struct Tag {
    owner: BitRepo,
    cached: BitObjCached,
    inner: MutableTag,
}

#[derive(PartialEq, Clone, Debug)]
pub struct MutableTag {}

impl Serialize for MutableTag {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl Deserialize for MutableTag {
    fn deserialize(mut _reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl Deref for Tag {
    type Target = MutableTag;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BitObject for Tag {
    fn obj_cached(&self) -> &BitObjCached {
        todo!()
    }

    fn owner(&self) -> BitRepo {
        self.owner.clone()
    }
}

impl ImmutableBitObject for Tag {
    type Mutable = MutableTag;

    fn from_mutable(owner: BitRepo, cached: BitObjCached, inner: Self::Mutable) -> Self {
        Self { owner, cached, inner }
    }
}
