use crate::cli::cli_reflog::BitReflogShowOpts;

use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;

// default subcommand's are a bit awkward, not sure how to do this nicely
#[derive(Clap, Debug)]
pub struct BitRemoteCliOpts {
    reference: Option<BitRef>,
    #[clap(subcommand)]
    subcmd: Option<BitRemoteSubcommand>,
}

#[derive(Clap, Debug)]
pub enum BitRemoteSubcommand {
    Add(BitRemoteAddOpts),
    Remove(BitRemoteAddOpts),
    Show(BitRemoteShowOpts),
}

#[derive(Clap, Default, Debug)]
pub struct BitRemoteShowOpts {
    name: Option<String>,
}

#[derive(Clap, Debug)]
pub struct BitRemoteAddOpts {
    remote: String,
    url: String,
}

#[derive(Clap, Debug)]
pub struct BitRemoteRemoveOpts {
    remote: String,
}

impl Cmd for BitRemoteCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        match self.subcmd {
            Some(subcmd) => match subcmd {
                BitRemoteSubcommand::Add(opts) => todo!(),
                BitRemoteSubcommand::Remove(_) => todo!(),
                BitRemoteSubcommand::Show(show_opts) => show_opts.exec(repo),
            },
            None => BitRemoteShowOpts::default().exec(repo),
        }
    }
}

impl Cmd for BitRemoteShowOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        match self.name {
            Some(_) => todo!(),
            None => repo.ls_remotes().for_each(|remote| println!("{}", remote.name)),
        }
        Ok(())
    }
}
