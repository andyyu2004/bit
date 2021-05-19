use clap::Clap;
use libbit::cmd::BitLsFilesOpts;

#[derive(Clap, Debug)]
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
