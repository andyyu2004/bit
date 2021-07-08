use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitCheckoutCliOpts {
    revision: LazyRevspec,
}

impl Cmd for BitCheckoutCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        repo.checkout_rev(&self.revision)
    }
}
