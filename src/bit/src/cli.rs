mod cli_add;
mod cli_commit;
mod cli_commit_tree;
mod cli_config;
mod cli_ls_files;
mod cli_status;
mod cli_update_index;

use self::cli_add::BitAddCliOpts;
// the bitopts and bitcliopts are distinct types for a few reasons
// - the parsed format is often not very convenient for actual usage
// - feels a bit (punny!) wrong to have cli parsing stuff in the library
// - probably will make it such that libbit doesn't even expose full commands
//   and be something more like libgit2
use cli_commit::BitCommitCliOpts;
use cli_commit_tree::BitCommitTreeCliOpts;
use cli_config::BitConfigCliOpts;
use cli_ls_files::BitLsFilesCliOpts;
use cli_status::BitStatusCliOpts;
use cli_update_index::BitUpdateIndexCliOpts;

use clap::{AppSettings, Clap};
use libbit::cmd::*;
use libbit::error::BitResult;
use libbit::obj::BitObjType;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;
use std::path::PathBuf;

pub fn run() -> BitResult<()> {
    let opts = BitCliOpts::parse();
    let BitCliOpts { subcmd, root_path } = opts;
    if let BitSubCmd::Init(subcmd) = &subcmd {
        BitRepo::init(root_path.join(&subcmd.path))?;
        return Ok(());
    }

    BitRepo::find(root_path, |repo| match subcmd {
        BitSubCmd::Log(..) => todo!(),
        BitSubCmd::Init(..) => unreachable!(),
        // TODO the real behaviour is more complex than this
        BitSubCmd::Add(opts) =>
            if opts.dryrun {
                repo.bit_add_dryrun(&opts.pathspecs)
            } else if opts.all {
                repo.bit_add_all()
            } else {
                repo.bit_add(&opts.pathspecs)
            },
        BitSubCmd::HashObject(opts) => repo.bit_hash_object(opts.into()),
        BitSubCmd::WriteTree => repo.bit_write_tree(),
        BitSubCmd::CatFile(opts) => repo.bit_cat_file(opts.into()),
        BitSubCmd::LsFiles(opts) => repo.bit_ls_files(opts.into()),
        BitSubCmd::Config(opts) => opts.execute(repo),
        BitSubCmd::UpdateIndex(opts) => {
            dbg!(opts);
            todo!()
        }
        BitSubCmd::Status(_opts) => Ok(print!("{}", repo.status_report()?)),
        BitSubCmd::CommitTree(opts) => repo.bit_commit_tree(opts.parent, opts.message, opts.tree),
        BitSubCmd::Commit(opts) => repo.bit_commit(opts.message),
    })
}

#[derive(Clap, Debug)]
#[clap(author = "Andy Yu <andyyu2004@gmail.com>")]
pub struct BitCliOpts {
    #[clap(subcommand)]
    pub subcmd: BitSubCmd,
    #[clap(short = 'C', default_value = ".")]
    pub root_path: PathBuf,
}

#[derive(Clap, Debug)]
pub enum BitSubCmd {
    Add(BitAddCliOpts),
    CatFile(BitCatFileCliOpts),
    CommitTree(BitCommitTreeCliOpts),
    Config(BitConfigCliOpts),
    Commit(BitCommitCliOpts),
    HashObject(BitHashObjectCliOpts),
    Init(BitInitCliOpts),
    Log(BitLogCliOpts),
    LsFiles(BitLsFilesCliOpts),
    Status(BitStatusCliOpts),
    UpdateIndex(BitUpdateIndexCliOpts),
    WriteTree,
}

#[derive(Clap, Debug)]
pub struct BitInitCliOpts {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

impl Into<BitInitOpts> for BitInitCliOpts {
    fn into(self) -> BitInitOpts {
        let Self { path } = self;
        BitInitOpts { path }
    }
}

#[derive(Clap, Debug)]
pub struct BitLogCliOpts {
    #[clap(default_value = ".")]
    pub commit: PathBuf,
}

// bit hash-object [-w] [-t TYPE] PATH
#[derive(Clap, Debug)]
pub struct BitHashObjectCliOpts {
    #[clap(short = 'w')]
    pub do_write: bool,
    #[clap(default_value = "blob", short = 't', long = "type")]
    pub objtype: BitObjType,
    pub path: PathBuf,
}

impl Into<BitHashObjectOpts> for BitHashObjectCliOpts {
    fn into(self) -> BitHashObjectOpts {
        let Self { do_write, objtype, path } = self;
        BitHashObjectOpts { do_write, objtype, path }
    }
}

// bit cat-file (-t | -s | -p | -e | <type>) <object>
#[derive(Clap, Debug)]
#[clap(setting = AppSettings::AllowMissingPositional)]
pub struct BitCatFileCliOpts {
    /// pretty print object
    #[clap(short = 'p', conflicts_with_all(&["size", "ty", "objtype", "exit"]))]
    pub pp: bool,
    // exit with zero status if <object> exists and is valid. If <object> is of an invalid format
    // then exit with non-zero status and emit an error on stderr
    #[clap(short = 'e', conflicts_with_all(&["size", "ty", "objtype"]))]
    pub exit: bool,
    /// show object type
    #[clap(short = 't', conflicts_with_all(&["size", "objtype"]))]
    pub ty: bool,
    /// show object size
    #[clap(short = 's', conflicts_with("objtype"))]
    pub size: bool,
    #[clap(required_unless_present_any(&["pp", "ty", "size", "exit"]))]
    pub objtype: Option<BitObjType>,
    #[clap(required = true)]
    pub object: BitRef,
}

impl Into<BitCatFileOpts> for BitCatFileCliOpts {
    fn into(self) -> BitCatFileOpts {
        let Self { pp, exit, ty, size, objtype, object } = self;
        let op = if pp {
            BitCatFileOperation::PrettyPrint
        } else if size {
            BitCatFileOperation::ShowSize
        } else if exit {
            BitCatFileOperation::Exit
        } else if ty {
            BitCatFileOperation::ShowType
        } else {
            BitCatFileOperation::PrintAsType(objtype.unwrap())
        };
        BitCatFileOpts { object, op }
    }
}
