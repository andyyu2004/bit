use crate::error::BitResult;
use crate::obj::{BitObjId, BitObjKind, BitObjType};
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
    pub fn bit_cat_file(&self, opts: BitCatFileOpts) -> BitResult<BitObjKind> {
        dbg!(&opts);
        match opts.op {
            BitCatFileOperation::PrintAsType(_) => {}
            BitCatFileOperation::ShowType => {}
            BitCatFileOperation::ShowSize => {}
            BitCatFileOperation::Exit => {}
            BitCatFileOperation::PrettyPrint => {}
        }
        let hash = self.get_full_object_hash(opts.object)?;
        self.read_obj_from_hash(&hash)
    }
}

