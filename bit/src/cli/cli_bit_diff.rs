use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitDiffCliOpts {}

impl Cmd for BitDiffCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        Ok(())
    }
}
