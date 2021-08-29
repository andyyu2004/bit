use crate::error::BitResult;
use crate::obj::{BitObjType, MutableBlob, Oid, WritableObject};
use crate::repo::BitRepo;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BitHashObjectOpts {
    pub objtype: BitObjType,
    pub do_write: bool,
    pub path: PathBuf,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_hash_object(&self, opts: BitHashObjectOpts) -> BitResult<()> {
        let hash = self.hash_object(opts)?;
        println!("{}", hash);
        Ok(())
    }

    pub fn hash_object(&self, opts: BitHashObjectOpts) -> BitResult<Oid> {
        let path = opts.path.canonicalize()?;
        let reader = BufReader::new(File::open(&path)?);
        let obj = match opts.objtype {
            BitObjType::Tree => todo!(),
            BitObjType::Tag => todo!(),
            BitObjType::Commit => todo!(),
            //BitObjKind::Commit(Commit::deserialize_to_end(reader)?),
            BitObjType::Blob => Box::new(MutableBlob::from_reader(reader)?),
        };

        let obj: &dyn WritableObject = obj.as_ref();

        if opts.do_write { self.write_obj(obj) } else { obj.hash() }
    }
}
