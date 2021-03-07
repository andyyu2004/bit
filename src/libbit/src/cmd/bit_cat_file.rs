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
        let stream = self.obj_stream_from_hash(&hash)?;
        match opts.op {
            // TODO not really a correct implementation currently
            // just prints it in an alternate format
            BitCatFileOperation::PrintAsType(_ty) => print!("{:#}", obj::read_obj(stream)?),
            BitCatFileOperation::ShowType => println!("{}", obj::read_obj_type(stream)?),
            BitCatFileOperation::ShowSize => println!("{}", obj::read_obj_size_from_start(stream)?),
            BitCatFileOperation::PrettyPrint => {
                println!("{}", obj::read_obj(stream)?);
            }
            // just try to read the file and if it sulceeds then its fine
            BitCatFileOperation::Exit => {
                obj::read_obj(stream)?;
            }
        }
        Ok(())
    }
}

