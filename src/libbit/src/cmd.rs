use crate::error::BitResult;
use crate::repo::BitRepo;
use std::path::Path;

pub struct BitInitOpts<'a> {
    pub path: &'a Path,
}

pub fn init<'a>(opts: BitInitOpts<'a>) -> BitResult<()> {
    let _repo = BitRepo::init(opts.path)?;
    Ok(())
}
