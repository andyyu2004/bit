use crate::error::BitResult;
use crate::obj::BitObjType;
use crate::repo::BitRepo;
use crate::rev::LazyRevspec;

#[derive(Debug)]
pub struct BitCatFileOpts {
    pub rev: LazyRevspec,
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

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_cat_file(&self, opts: BitCatFileOpts) -> BitResult<()> {
        trace!("bit_cat_file {:?}", opts);
        let id = self.fully_resolve_rev(&opts.rev)?;
        match opts.op {
            // TODO not really a correct implementation currently
            // just prints it in an alternate format
            BitCatFileOperation::PrintAsType(_ty) => println!("{:#}", self.read_obj(id)?),
            BitCatFileOperation::PrettyPrint => println!("{}", self.read_obj(id)?),
            BitCatFileOperation::ShowType => println!("{}", self.read_obj_header(id)?.obj_type),
            BitCatFileOperation::ShowSize => println!("{}", self.read_obj_header(id)?.size),
            // just try to read the file and if it suceeds then its fine
            BitCatFileOperation::Exit => {
                self.read_obj(id)?;
            }
        }
        Ok(())
    }
}
