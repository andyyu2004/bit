use crate::error::BitResult;
use crate::obj::{BitObjId, BitObjType};
use crate::repo::BitRepo;
use clap::{AppSettings, Clap};
use std::path::PathBuf;

pub fn main() {
    if let Err(err) = run() {
        eprintln!("{:?}", err)
    }
}

pub fn run() -> BitResult<()> {
    let opts: BitOpts = BitOpts::parse();
    let root_path = &opts.root_path;
    if let BitSubCmds::Init(opts) = &opts.subcmd {
        BitRepo::init(root_path.join(&opts.path))?;

        return Ok(());
    }

    let repo = BitRepo::find(root_path)?;
    match opts.subcmd {
        BitSubCmds::HashObject(opts) => {
            let should_write = opts.write;
            let hash = repo.bit_hash_object(opts)?;
            if !should_write {
                println!("{}", hash)
            }
            Ok(())
        }
        BitSubCmds::CatFile(opts) => {
            let obj = repo.bit_cat_file(opts)?;
            print!("{}", obj);
            Ok(())
        }
        BitSubCmds::Log(..) => todo!(),
        BitSubCmds::Init(..) => unreachable!(),
    }
}

#[derive(Clap)]
#[clap(author = "Andy Yu <andyyu2004@gmail.com>")]
pub struct BitOpts {
    #[clap(subcommand)]
    pub subcmd: BitSubCmds,
    #[clap(short = 'C', default_value = ".")]
    pub root_path: PathBuf,
}

#[derive(Clap)]
pub enum BitSubCmds {
    Init(BitInitOpts),
    HashObject(BitHashObjectOpts),
    CatFile(BitCatFileOpts),
    Log(BitLogOpts),
}

#[derive(Clap)]
pub struct BitInitOpts {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

#[derive(Clap)]
pub struct BitLogOpts {
    #[clap(default_value = ".")]
    pub commit: PathBuf,
}

/// bit hash-object [-w] [-t TYPE] PATH
#[derive(Clap, Debug)]
pub struct BitHashObjectOpts {
    #[clap(short = 'w')]
    pub write: bool,
    #[clap(default_value = "blob", short = 't', long = "type", requires_if("out", "hello"))]
    pub objtype: BitObjType,
    pub path: PathBuf,
}

#[derive(Clap, Debug)]
#[clap(setting = AppSettings::AllowMissingPositional)]
pub struct BitCatFileOpts {
    /// pretty print object
    #[clap(short = 'p', conflicts_with_all(&["size", "ty", "objtype"]))]
    pub pp: bool,
    /// show object type
    #[clap(short = 't', conflicts_with_all(&["size", "objtype"]))]
    pub ty: bool,
    /// show object size
    #[clap(short = 's', conflicts_with("objtype"))]
    pub size: bool,
    #[clap(required_unless_present_any(&["pp", "ty", "size"]))]
    pub objtype: Option<BitObjType>,
    #[clap(required = true)]
    pub id: BitObjId,
}

