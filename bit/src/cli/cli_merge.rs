use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitMergeCliOpts {
    revision: Revspec,
}

impl Cmd for BitMergeCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        repo.merge(&self.revision)?;
        Ok(())
    }
}
