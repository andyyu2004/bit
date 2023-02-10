use clap::Parser;
use libbit::cmd::{BitUpdateIndexOpts, CacheInfo};
use libbit::error::BitGenericError;

#[derive(Parser, Debug)]
pub struct BitUpdateIndexCliOpts {
    #[clap(long = "add")]
    add: bool,
    #[clap(long = "cacheinfo", use_value_delimiter = true, number_of_values = 3)]
    cacheinfo: Vec<String>,
}

impl TryInto<BitUpdateIndexOpts> for BitUpdateIndexCliOpts {
    type Error = BitGenericError;

    fn try_into(self) -> Result<BitUpdateIndexOpts, Self::Error> {
        let Self { add, mut cacheinfo } = self;

        let cacheinfo = CacheInfo {
            mode: cacheinfo[0].parse()?,
            hash: cacheinfo[1].parse()?,
            path: std::mem::take(&mut cacheinfo[2]),
        };

        Ok(BitUpdateIndexOpts { add, cacheinfo })
    }
}
