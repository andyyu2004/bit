use super::Cmd;
use clap::Parser;
use libbit::checkout::{CheckoutOpts, CheckoutStrategy};
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Parser, Debug)]
pub struct BitCheckoutCliOpts {
    #[arg(default_value = "HEAD")]
    revision: Revspec,
    #[arg(short = 'f', long = "--force")]
    force: bool,
}

impl Cmd for BitCheckoutCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
        let mut opts = CheckoutOpts::default();
        if self.force {
            opts.strategy = CheckoutStrategy::Force;
        }
        repo.checkout_revision(&self.revision, opts)?;
        Ok(())
    }
}
