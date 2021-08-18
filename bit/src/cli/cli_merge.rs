use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitMergeCliOpts {
    revision: LazyRevspec,
}

impl Cmd for BitMergeCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let oid = repo.fully_resolve_rev(&self.revision)?;
        repo.merge(oid)?;
        Ok(())
    }
}
