use super::Cmd;
use clap::Clap;
use libbit::checkout::{CheckoutOpts, CheckoutStrategy};
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitCheckoutCliOpts {
    revision: Revspec,
    #[clap(short = 'f', long = "--force")]
    force: bool,
}

impl Cmd for BitCheckoutCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let mut opts = CheckoutOpts::default();
        if self.force {
            opts.strategy = CheckoutStrategy::Force;
        }
        repo.checkout_revision(&self.revision, opts)
    }
}
