use crate::cli::*;
use crate::error::BitResult;
use crate::obj::{self, BitObjKind, Blob};
use crate::repo::BitRepo;
use std::fs::File;
use std::io::Read;

pub fn bit_init(opts: BitInitOpts) -> BitResult<()> {
    let _repo = BitRepo::init(&opts.path)?;
    Ok(())
}

impl BitRepo {
    pub fn bit_hash_object(&self, opts: &BitHashObjectOpts) -> BitResult<String> {
        let mut buf = vec![];
        let path = opts.path.canonicalize()?;
        File::open(&path)?.read_to_end(&mut buf)?;
        let object = match opts.objtype {
            obj::BitObjType::Commit => todo!(),
            obj::BitObjType::Tree => todo!(),
            obj::BitObjType::Tag => todo!(),
            obj::BitObjType::Blob => BitObjKind::Blob(Blob::new(buf)),
        };

        if opts.write { self.write_obj(&object) } else { obj::hash_obj(&object) }
    }

    pub fn bit_cat_file(&self, opts: &BitCatFileOpts) -> BitResult<BitObjKind> {
        let id = self.find_obj(&opts.name)?;
        self.read_obj_from_hash(&id)
    }
}
