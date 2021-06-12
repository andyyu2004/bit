use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;

// default subcommand's are a bit awkward, not sure how to do this nicely
#[derive(Clap, Debug)]
pub struct BitReflogCliOpts {
    reference: Option<BitRef>,
    #[clap(subcommand)]
    subcmd: Option<BitReflogSubcommand>,
}

#[derive(Clap, Debug)]
pub enum BitReflogSubcommand {
    Show(BitReflogShowOpts),
}

#[derive(Clap, Debug)]
pub struct BitReflogShowOpts {
    #[clap(default_value = "HEAD")]
    reference: BitRef,
}

impl Cmd for BitReflogCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        if let Some(subcmd) = self.subcmd {
            match subcmd {
                BitReflogSubcommand::Show(opts) => opts.exec(repo),
            }
        } else {
            // show opts exec
            BitReflogShowOpts { reference: self.reference.unwrap() }.exec(repo)
        }
    }
}

impl Cmd for BitReflogShowOpts {
    fn exec(self, _repo: BitRepo<'_>) -> BitResult<()> {
        todo!()
    }
}
