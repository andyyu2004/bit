use clap::Clap;
use libbit::pathspec::Pathspec;

#[derive(Clap)]
pub struct BitAddCliOpts {
    pathspec: Pathspec,
}
