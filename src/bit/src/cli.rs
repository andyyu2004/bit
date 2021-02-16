use clap::Clap;
use std::path::PathBuf;

#[derive(Clap)]
#[clap(author = "Andy Yu <andyyu2004@gmail.com>")]
pub struct BitOpts {
    #[clap(subcommand)]
    pub subcmd: BitSubCmds,
}

#[derive(Clap)]
pub enum BitSubCmds {
    Init(BitInit),
}

#[derive(Clap)]
pub struct BitInit {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}
