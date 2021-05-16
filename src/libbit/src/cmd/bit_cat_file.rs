use crate::error::BitResult;
use crate::obj::BitObjType;
use crate::refs::BitRef;
use crate::repo::BitRepo;

#[derive(Debug)]
pub struct BitCatFileOpts {
    pub object: BitRef,
    pub op: BitCatFileOperation,
}

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
        let id = self.fully_resolve_ref(opts.object)?;
        match opts.op {
            // TODO not really a correct implementation currently
            // just prints it in an alternate format
            BitCatFileOperation::PrintAsType(_ty) => print!("{:#}", self.read_obj(id)?),
            BitCatFileOperation::PrettyPrint => print!("{}", self.read_obj(id)?),
            BitCatFileOperation::ShowType => print!("{}", self.read_obj_header(id)?.obj_type),
            BitCatFileOperation::ShowSize => print!("{}", self.read_obj_header(id)?.size),
            // just try to read the file and if it suceeds then its fine
            BitCatFileOperation::Exit => {
                self.read_obj(id)?;
            }
        }
        Ok(())
    }
}
