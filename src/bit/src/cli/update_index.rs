use clap::Clap;
use libbit::cmd::{BitUpdateIndexOpts, CacheInfo};
use libbit::error::BitError;
use std::convert::TryInto;

#[derive(Clap, Debug)]
pub struct BitUpdateIndexCliOpts {
    #[clap(long = "add")]
    add: bool,
    #[clap(long = "cacheinfo", use_delimiter = true)]
    cacheinfo: Vec<String>,
}

impl TryInto<BitUpdateIndexOpts> for BitUpdateIndexCliOpts {
    type Error = BitError;

    fn try_into(self) -> Result<BitUpdateIndexOpts, Self::Error> {
        let Self { add, mut cacheinfo } = self;
        if cacheinfo.len() != 3 {
            return Err(BitError::Msg(format!(
                "option 'cacheinfo' expects arguments `<mode>,<sha1>,<path>`"
            )));
        }

        let cacheinfo = CacheInfo {
            mode: cacheinfo[0].parse()?,
            hash: cacheinfo[1].parse()?,
            path: String::from(std::mem::take(&mut cacheinfo[2])),
        };

        Ok(BitUpdateIndexOpts { add, cacheinfo })
    }
}
