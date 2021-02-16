mod cli;

use clap::Clap;
use cli::{BitOpts, BitSubCmds};
use libbit::{cmd, BitResult};

fn main() -> BitResult<()> {
    let opts: BitOpts = BitOpts::parse();
    match opts.subcmd {
        BitSubCmds::Init(initopts) => cmd::init(cmd::BitInitOpts { path: &initopts.path }),
    }
}
