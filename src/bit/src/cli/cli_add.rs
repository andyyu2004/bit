use clap::Clap;
use libbit::pathspec::Pathspec;

#[derive(Clap)]
pub struct BitAddCliOpts {
    #[clap(multiple = true, required = true)]
    pub pathspecs: Vec<Pathspec>,
    #[clap(short = 'n', long = "dry-run")]
    pub dryrun: bool,
    #[clap(short = 'A', long = "all")]
    pub all: bool,
}
