use clap::Parser;
use libbit::pathspec::Pathspec;

#[derive(Parser, Debug)]
pub struct BitAddCliOpts {
    #[arg(num_args=0.., required_unless_present("all"))]
    pub pathspecs: Vec<Pathspec>,
    #[arg(short = 'n', long = "dry-run")]
    pub dryrun: bool,
    #[arg(short = 'A', long = "all")]
    pub all: bool,
}
