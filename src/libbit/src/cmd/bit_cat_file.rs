use crate::error::BitResult;
use crate::obj::{self, BitObjId, BitObjType};
use crate::repo::BitRepo;

#[derive(Debug)]
pub struct BitCatFileOpts {
    pub object: BitObjId,
    pub op: BitCatFileOperation,
}

/// check docs on [`BitCatFileCliOpts`]
#[derive(Debug)]
pub enum BitCatFileOperation {
    PrintAsType(BitObjType),
    ShowType,
    ShowSize,
    Exit,
    PrettyPrint,
}

impl BitRepo {
    pub fn bit_cat_file(&self, opts: BitCatFileOpts) -> BitResult<()> {
        let hash = self.get_full_object_hash(opts.object)?;
        let file = self.obj_stream_from_hash(&hash)?;
        match opts.op {
            BitCatFileOperation::PrintAsType(_) => todo!(),
            BitCatFileOperation::ShowType => println!("{}", obj::read_obj_type(file)?),
            BitCatFileOperation::ShowSize => println!("{}", obj::read_obj_size_from_start(file)?),
            BitCatFileOperation::PrettyPrint => {
                let obj = obj::read_obj(file)?;
            }
            BitCatFileOperation::Exit => {
                // just try to read the file and if it succeeds then its fine
                obj::read_obj(file)?;
            }
        }
        Ok(())
    }
}

