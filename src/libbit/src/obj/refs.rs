use super::{BitObj, BitObjKind, BitObjType};
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::repo::BitRepo;
use std::fmt::{self, Display, Formatter};
use std::io::{Read, Write};
use std::path::PathBuf;

impl Display for Ref {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub enum Ref {
    /// refers directly to an object
    Direct(BitHash),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `PathBuf` contains `refs/remote/origin/master`
    Indirect(PathBuf),
}

impl BitObj for Ref {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        todo!()
    }

    fn deserialize<R: Read>(reader: R) -> BitResult<Self> {
        todo!()
    }

    fn obj_ty(&self) -> BitObjType {
        todo!()
    }
}

impl BitRepo {
    pub fn resolve_ref(&self, r: &Ref) -> BitResult<BitObjKind> {
        match r {
            Ref::Direct(hash) => self.read_obj_from_hash(hash),
            Ref::Indirect(path) => {
                let r = todo!();
                // self.resolve_ref(r)
            }
        }
    }
}
