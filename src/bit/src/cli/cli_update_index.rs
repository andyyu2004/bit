use clap::Clap;
use libbit::cmd::{BitUpdateIndexOpts, CacheInfo};
use libbit::error::BitGenericError;
use std::convert::TryInto;

#[derive(Clap, Debug)]
pub struct BitUpdateIndexCliOpts {
    #[clap(long = "add")]
    add: bool,
    #[clap(long = "cacheinfo", use_delimiter = true)]
    cacheinfo: Vec<String>,
}

impl TryInto<BitUpdateIndexOpts> for BitUpdateIndexCliOpts {
    type Error = BitGenericError;

    fn try_into(self) -> Result<BitUpdateIndexOpts, Self::Error> {
        let Self { add, mut cacheinfo } = self;
        if cacheinfo.len() != 3 {
            bail!("option 'cacheinfo' expects arguments `<mode>,<sha1>,<path>`");
        }

        let cacheinfo = CacheInfo {
            mode: cacheinfo[0].parse()?,
            hash: cacheinfo[1].parse()?,
            path: std::mem::take(&mut cacheinfo[2]),
        };

        Ok(BitUpdateIndexOpts { add, cacheinfo })
    }
}
