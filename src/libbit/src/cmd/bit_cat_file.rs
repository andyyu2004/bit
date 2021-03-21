use crate::error::BitResult;
use crate::obj::{BitObjId, BitObjType};
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
        let id = opts.object;
        match opts.op {
            // TODO not really a correct implementation currently
            // just prints it in an alternate format
            BitCatFileOperation::PrintAsType(_ty) => print!("{:#}", self.read_obj_from_id(id)?),
            BitCatFileOperation::ShowType => print!("{}", self.read_obj_type_from_id(id)?),
            BitCatFileOperation::ShowSize => print!("{}", self.read_obj_size_from_id(id)?),
            BitCatFileOperation::PrettyPrint => {
                print!("{}", self.read_obj_from_id(id)?);
            }
            // just try to read the file and if it sulceeds then its fine
            BitCatFileOperation::Exit => {
                self.read_obj_from_id(id)?;
            }
        }
        Ok(())
    }
}
