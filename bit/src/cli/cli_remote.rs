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

#[derive(Clap, Debug)]
pub struct BitRemoteShowOpts {
    #[clap(default_value = "HEAD")]
    reference: BitRef,
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
                BitRemoteSubcommand::Show(_) => todo!(),
            },
            None => todo!("default command to show"),
        }
    }
}
