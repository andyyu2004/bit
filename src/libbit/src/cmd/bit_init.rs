use crate::error::BitResult;
use crate::repo::BitRepo;
use std::path::PathBuf;

#[derive(Debug)]
pub struct BitInitOpts {
    pub path: PathBuf,
}

pub fn bit_init(opts: BitInitOpts) -> BitResult<()> {
    let _repo = BitRepo::init(&opts.path)?;
    Ok(())
}

