use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::FileMode;
use crate::path::BitPath;
use crate::repo::BitRepo;

#[derive(Debug)]
pub struct BitUpdateIndexOpts {
    pub add: bool,
    pub cacheinfo: CacheInfo,
}

#[derive(Debug)]
pub struct CacheInfo {
    pub mode: FileMode,
    pub hash: BitHash,
    pub path: BitPath,
}

impl BitRepo {
    pub fn bit_update_index(&self, opts: BitUpdateIndexOpts) -> BitResult<()> {
        Ok(())
    }
}
