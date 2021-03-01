use crate::obj::BitObjType;
use crate::{BitRepo, BitResult};
use clap::Clap;
use std::path::PathBuf;

pub fn main() -> BitResult<()> {
    let opts: BitOpts = BitOpts::parse();
    let root_path = &opts.root_path;
    if let BitSubCmds::Init(opts) = &opts.subcmd {
        BitRepo::init(root_path.join(&opts.path))?;
        return Ok(());
    }

    let repo = BitRepo::init(root_path)?;
    match opts.subcmd {
        BitSubCmds::HashObject(opts) => {
            let hash = repo.bit_hash_object(&opts)?;
            if !opts.write {
                println!("{}", hash)
            }
            Ok(())
        }
        BitSubCmds::CatFile(opts) => {
            let repo = BitRepo::init(root_path)?;
            let obj = repo.bit_cat_file(&opts)?;
            println!("{}", obj);
            Ok(())
        }
        BitSubCmds::Init(..) => unreachable!(),
    }
}

#[derive(Clap)]
#[clap(author = "Andy Yu <andyyu2004@gmail.com>")]
pub struct BitOpts {
    #[clap(subcommand)]
    pub subcmd: BitSubCmds,
    #[clap(short = 'C')]
    pub root_path: PathBuf,
}

#[derive(Clap)]
pub enum BitSubCmds {
    Init(BitInitOpts),
    HashObject(BitHashObjectOpts),
    CatFile(BitCatFileOpts),
}

#[derive(Clap)]
pub struct BitInitOpts {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

/// bit hash-object [-w] [-t TYPE] PATH
#[derive(Clap)]
pub struct BitHashObjectOpts {
    #[clap(short = 'w')]
    pub write: bool,
    #[clap(default_value = "blob", short = 't', long = "type")]
    pub objtype: BitObjType,
    pub path: PathBuf,
}

#[derive(Clap)]
pub struct BitCatFileOpts {
    pub objtype: BitObjType,
    pub name: String,
}
