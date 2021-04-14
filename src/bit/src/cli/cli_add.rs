use clap::Clap;
use libbit::cmd::BitAddOpts;
use libbit::pathspec::Pathspec;

#[derive(Clap)]
pub struct BitAddCliOpts {
    #[clap(multiple = true, required = true)]
    pathspecs: Vec<Pathspec>,
    #[clap(short = 'n', long = "dry-run")]
    dryrun: bool,
}

impl Into<BitAddOpts> for BitAddCliOpts {
    fn into(self) -> BitAddOpts {
        let Self { pathspecs, dryrun } = self;
        BitAddOpts { pathspecs, dryrun }
    }
}
