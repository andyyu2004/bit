use crate::cli::*;
use crate::error::BitResult;
use crate::hash::{self, SHA1Hash};
use crate::obj::{self, BitObjKind, Blob, Commit};
use crate::repo::BitRepo;
use std::fs::File;

pub fn bit_init(opts: BitInitOpts) -> BitResult<()> {
    let _repo = BitRepo::init(&opts.path)?;
    Ok(())
}

impl BitRepo {
    pub fn bit_hash_object(&self, opts: BitHashObjectOpts) -> BitResult<SHA1Hash> {
        let path = opts.path.canonicalize()?;
        let reader = File::open(&path)?;
        let object = match opts.objtype {
            obj::BitObjType::Tree => todo!(),
            obj::BitObjType::Tag => todo!(),
            obj::BitObjType::Commit => BitObjKind::Commit(Commit::parse(reader)?),
            obj::BitObjType::Blob => BitObjKind::Blob(Blob::from_reader(reader)?),
        };

        if opts.write { self.write_obj(&object) } else { hash::hash_obj(&object) }
    }

    pub fn bit_cat_file(&self, opts: BitCatFileOpts) -> BitResult<BitObjKind> {
        let hash = self.find_obj(opts.id)?;
        self.read_obj_from_hash(&hash)
    }
}
