use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::FileMode;
use crate::repo::BitRepo;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BitUpdateIndexOpts {
    pub add: bool,
    pub cacheinfo: CacheInfo,
}

#[derive(Debug)]
pub struct CacheInfo {
    pub mode: FileMode,
    pub hash: BitHash,
    pub path: PathBuf,
}

impl BitRepo {
    pub fn bit_update_index(&self, opts: BitUpdateIndexOpts) -> BitResult<()> {
        Ok(())
    }
}
