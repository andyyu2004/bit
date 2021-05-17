use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitBranchCliOpts {
    name: String,
}

impl Cmd for BitBranchCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        repo.bit_create_branch(&self.name)?;
        Ok(())
    }
}
