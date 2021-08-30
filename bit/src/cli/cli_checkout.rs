use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitCheckoutCliOpts {
    revision: Revspec,
}

impl Cmd for BitCheckoutCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        repo.checkout_revision(&self.revision)
    }
}
