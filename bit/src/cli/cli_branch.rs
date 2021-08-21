use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitBranchCliOpts {
    name: String,
    #[clap(default_value = "HEAD")]
    revision: Revspec,
}

impl Cmd for BitBranchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        repo.bit_create_branch(&self.name, &self.revision)?;
        Ok(())
    }
}
