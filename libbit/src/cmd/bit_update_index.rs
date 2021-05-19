use crate::error::BitResult;
use crate::obj::{FileMode, Oid};
use crate::repo::BitRepo;

#[derive(Debug)]
pub struct BitUpdateIndexOpts {
    pub add: bool,
    pub cacheinfo: CacheInfo,
}

#[derive(Debug)]
pub struct CacheInfo {
    pub mode: FileMode,
    pub hash: Oid,
    pub path: String,
}

impl BitRepo {
    pub fn bit_update_index(&self, _opts: BitUpdateIndexOpts) -> BitResult<()> {
        Ok(())
    }
}
