use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitSwitchCliOpts {
    revision: LazyRevspec,
}

impl Cmd for BitSwitchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        // switch is currently a limited form of checkout where only branches are allowed (can't checkout commits)
        let branch = repo.resolve_rev_to_branch(&self.revision)?;
        repo.checkout_reference(branch)
    }
}
