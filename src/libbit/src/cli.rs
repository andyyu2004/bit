use crate::obj::BitObjTag;
use crate::{cmd, BitResult};
use clap::Clap;
use std::path::PathBuf;

pub fn main() -> BitResult<()> {
    let opts: BitOpts = BitOpts::parse();
    match opts.subcmd {
        BitSubCmds::Init(opts) => cmd::bit_init(opts),
        BitSubCmds::HashObject(opts) => cmd::bit_hash_object(opts),
        BitSubCmds::CatFile(opts) => cmd::bit_cat_file(opts),
    }
}

#[derive(Clap)]
#[clap(author = "Andy Yu <andyyu2004@gmail.com>")]
pub struct BitOpts {
    #[clap(subcommand)]
    pub subcmd: BitSubCmds,
}

#[derive(Clap)]
pub enum BitSubCmds {
    Init(BitInit),
    HashObject(BitHashObject),
    CatFile(BitCatFile),
}

#[derive(Clap)]
pub struct BitInit {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

/// bit hash-object [-w] [-t TYPE] PATH
#[derive(Clap)]
pub struct BitHashObject {
    #[clap(short = 'w')]
    pub write: bool,
    #[clap(default_value = "blob", short = 't', long = "type")]
    pub objtype: BitObjTag,
    pub path: PathBuf,
}

#[derive(Clap)]
pub struct BitCatFile {
    pub objtype: BitObjTag,
    pub name: String,
}
