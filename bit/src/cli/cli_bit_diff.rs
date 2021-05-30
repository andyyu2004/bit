use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitDiffCliOpts {
    #[clap(long = "staged", default_missing_value = "HEAD")]
    staged: Option<BitRef>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        dbg!(self);
        Ok(())
    }
}
