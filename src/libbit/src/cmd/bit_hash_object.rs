use crate::error::BitResult;
use crate::hash::{self, BitHash};
use crate::obj::{self, BitObjKind, BitObjType, Blob, Commit};
use crate::repo::BitRepo;
use crate::serialize::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BitHashObjectOpts {
    pub objtype: BitObjType,
    pub do_write: bool,
    pub path: PathBuf,
}

impl BitRepo {
    pub fn bit_hash_object(&self, opts: BitHashObjectOpts) -> BitResult<()> {
        let hash = self.hash_object(opts)?;
        println!("{}", hash);
        Ok(())
    }

    pub fn hash_object(&self, opts: BitHashObjectOpts) -> BitResult<BitHash> {
        let path = opts.path.canonicalize()?;
        let reader = &mut BufReader::new(File::open(&path)?);
        let object = match opts.objtype {
            obj::BitObjType::Tree => todo!(),
            obj::BitObjType::Tag => todo!(),
            obj::BitObjType::Commit => BitObjKind::Commit(Commit::deserialize(reader)?),
            obj::BitObjType::Blob => BitObjKind::Blob(Blob::from_reader(reader)?),
        };

        if opts.do_write { self.write_obj(&object) } else { hash::hash_obj(&object) }
    }
}
