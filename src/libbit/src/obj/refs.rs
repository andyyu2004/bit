use super::{BitObj, BitObjKind, BitObjType};
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::Serialize;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

impl Display for Ref {
    fn fmt(&self, _f: &mut Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Ref {
    /// refers directly to an object
    Direct(BitHash),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `BitPath` contains `refs/remote/origin/master`
    Indirect(BitPath),
}

impl Serialize for Ref {
    fn serialize(&self, _writer: &mut dyn Write) -> BitResult<()> {
        todo!()
    }
}

impl BitObj for Ref {
    fn deserialize<R: BufRead>(_reader: &mut R) -> BitResult<Self> {
        todo!()
    }

    fn obj_ty(&self) -> BitObjType {
        todo!()
    }
}

impl BitRepo {
    pub fn resolve_ref(&self, r: Ref) -> BitResult<BitObjKind> {
        match r {
            Ref::Direct(hash) => self.read_obj(hash),
            Ref::Indirect(_path) => {
                let _r = todo!();
                // self.resolve_ref(r)
            }
        }
    }
}
