use clap::Parser;
use libbit::cmd::BitLsFilesOpts;

#[derive(Parser, Debug)]
pub struct BitLsFilesCliOpts {
    #[clap(short = 's', long = "stage")]
    stage: bool,
}

impl From<BitLsFilesCliOpts> for BitLsFilesOpts {
    fn from(val: BitLsFilesCliOpts) -> Self {
        let BitLsFilesCliOpts { stage } = val;
        BitLsFilesOpts { stage }
    }
}
