use clap::Parser;
use libbit::cmd::BitLsFilesOpts;

#[derive(Parser, Debug)]
pub struct BitLsFilesCliOpts {
    #[clap(short = 's', long = "stage")]
    stage: bool,
}

impl Into<BitLsFilesOpts> for BitLsFilesCliOpts {
    fn into(self) -> BitLsFilesOpts {
        let Self { stage } = self;
        BitLsFilesOpts { stage }
    }
}
