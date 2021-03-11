use crate::error::BitResult;
use crate::hash::{self, BitHash};
use crate::obj::{self, BitObj, BitObjKind, BitObjType, Blob, Commit};
use crate::repo::BitRepo;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BitHashObjectOpts {
    pub objtype: BitObjType,
    pub do_write: bool,
    pub path: PathBuf,
}

impl BitRepo {
    pub fn bit_hash_object(&self, opts: BitHashObjectOpts) -> BitResult<BitHash> {
        let path = opts.path.canonicalize()?;
        let reader = File::open(&path)?;
        let object = match opts.objtype {
            obj::BitObjType::Tree => todo!(),
            obj::BitObjType::Tag => todo!(),
            obj::BitObjType::Commit => BitObjKind::Commit(Commit::deserialize(reader)?),
            obj::BitObjType::Blob => BitObjKind::Blob(Blob::from_reader(reader)?),
        };

        if opts.do_write { self.write_obj(&object) } else { hash::hash_obj(&object) }
    }
}
