use clap::Clap;
use libbit::pathspec::Pathspec;

#[derive(Clap, Debug)]
pub struct BitAddCliOpts {
    #[clap(multiple_values = true, required_unless_present("all"))]
    pub pathspecs: Vec<Pathspec>,
    #[clap(short = 'n', long = "dry-run")]
    pub dryrun: bool,
    #[clap(short = 'A', long = "all")]
    pub all: bool,
}
