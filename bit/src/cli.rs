mod cli_add;
mod cli_bit_diff;
mod cli_branch;
mod cli_checkout;
mod cli_commit;
mod cli_commit_tree;
mod cli_config;
mod cli_log;
mod cli_ls_files;
mod cli_merge;
mod cli_merge_base;
mod cli_reflog;
mod cli_reset;
mod cli_revlist;
mod cli_status;
mod cli_switch;
mod cli_update_index;

// notes
// the bitopts and bitcliopts are distinct types for a few reasons
// - the parsed format is often not very convenient for actual usage
// - feels a bit (punny!) wrong to have cli parsing stuff in the library
// - probably will make it such that libbit doesn't even expose full commands
//   and be something more like libgit2

use clap::{AppSettings, Clap};
use cli_add::BitAddCliOpts;
use cli_bit_diff::BitDiffCliOpts;
use cli_branch::*;
use cli_checkout::BitCheckoutCliOpts;
use cli_commit::BitCommitCliOpts;
use cli_commit_tree::BitCommitTreeCliOpts;
use cli_config::BitConfigCliOpts;
use cli_log::BitLogCliOpts;
use cli_ls_files::BitLsFilesCliOpts;
use cli_merge::BitMergeCliOpts;
use cli_merge_base::BitMergeBaseCliOpts;
use cli_reflog::BitReflogCliOpts;
use cli_reset::BitResetCliOpts;
use cli_revlist::BitRevlistCliOpts;
use cli_status::BitStatusCliOpts;
use cli_switch::BitSwitchCliOpts;
use cli_update_index::BitUpdateIndexCliOpts;
use libbit::cmd::*;
use libbit::error::BitResult;
use libbit::obj::BitObjType;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;
use std::ffi::OsString;
use std::path::PathBuf;

// experiment with changing structure of everything
// more code should be in the binary
// to much is in libbit I think
// see comment above
pub trait Cmd {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()>;
}

pub fn run<T: Into<OsString> + Clone>(args: impl IntoIterator<Item = T>) -> BitResult<()> {
    let opts = BitCliOpts::parse_from(args);
    let BitCliOpts { subcmd, root_path } = opts;
    if let BitSubCmd::Init(subcmd) = &subcmd {
        BitRepo::init(root_path.join(&subcmd.path))?;
        return Ok(());
    }

    BitRepo::find(root_path, |repo| match subcmd {
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
        BitSubCmd::Branch(opts) => opts.exec(repo),
        BitSubCmd::CatFile(opts) => repo.bit_cat_file(opts.into()),
        BitSubCmd::Checkout(opts) => opts.exec(repo),
        BitSubCmd::Config(opts) => opts.execute(repo),
        BitSubCmd::CommitTree(opts) =>
            repo.bit_commit_tree(opts.parents.into_iter().collect(), opts.message, opts.tree),
        BitSubCmd::Commit(opts) => opts.exec(repo),
        BitSubCmd::Diff(opts) => opts.exec(repo),
        BitSubCmd::HashObject(opts) => repo.bit_hash_object(opts.into()),
        BitSubCmd::Log(opts) => opts.exec(repo),
        BitSubCmd::LsFiles(opts) => repo.bit_ls_files(opts.into()),
        BitSubCmd::Merge(opts) => opts.exec(repo),
        BitSubCmd::MergeBase(opts) => opts.exec(repo),
        BitSubCmd::Reflog(opts) => opts.exec(repo),
        BitSubCmd::Reset(opts) => opts.exec(repo),
        BitSubCmd::Revlist(opts) => opts.exec(repo),
        BitSubCmd::Status(opts) => opts.exec(repo),
        BitSubCmd::Switch(opts) => opts.exec(repo),
        BitSubCmd::UpdateIndex(opts) => {
            dbg!(opts);
            todo!()
        }
        BitSubCmd::WriteTree => repo.bit_write_tree(),
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
    Branch(BitBranchCliOpts),
    CatFile(BitCatFileCliOpts),
    Checkout(BitCheckoutCliOpts),
    CommitTree(BitCommitTreeCliOpts),
    Config(BitConfigCliOpts),
    Commit(BitCommitCliOpts),
    Diff(BitDiffCliOpts),
    HashObject(BitHashObjectCliOpts),
    Init(BitInitCliOpts),
    Log(BitLogCliOpts),
    LsFiles(BitLsFilesCliOpts),
    Merge(BitMergeCliOpts),
    MergeBase(BitMergeBaseCliOpts),
    Reflog(BitReflogCliOpts),
    Reset(BitResetCliOpts),
    #[clap(name = "rev-list")]
    Revlist(BitRevlistCliOpts),
    Status(BitStatusCliOpts),
    Switch(BitSwitchCliOpts),
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
    pub revision: Revspec,
}

impl Into<BitCatFileOpts> for BitCatFileCliOpts {
    fn into(self) -> BitCatFileOpts {
        let Self { pp, exit, ty, size, objtype, revision: rev } = self;
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
        BitCatFileOpts { rev, op }
    }
}
