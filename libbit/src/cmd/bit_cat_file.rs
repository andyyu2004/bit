use crate::error::BitResult;
use crate::obj::BitObjType;
use crate::repo::BitRepo;
use crate::rev::Revspec;

#[derive(Debug)]
pub struct BitCatFileOpts {
    pub rev: Revspec,
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
        let oid = self.fully_resolve_rev_to_any(&opts.rev)?;
        match opts.op {
            // TODO not really a correct implementation currently
            // just prints it in an alternate format
            BitCatFileOperation::PrintAsType(_ty) => println!("{:#}", self.read_obj(oid)?),
            BitCatFileOperation::PrettyPrint => println!("{}", self.read_obj(oid)?),
            BitCatFileOperation::ShowType => println!("{}", self.read_obj_header(oid)?.obj_type),
            BitCatFileOperation::ShowSize => println!("{}", self.read_obj_header(oid)?.size),
            // just try to read the file and if it suceeds then its fine
            BitCatFileOperation::Exit => {
                self.read_obj(oid)?;
            }
        }
        Ok(())
    }
}
