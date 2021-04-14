use clap::Clap;
use libbit::hash::BitHash;

#[derive(Clap, Debug)]
pub struct BitCommitTreeCliOpts {
    #[clap(short = 'm', long = "message")]
    pub message: Option<String>,
    #[clap(short = 'p', long = "parent")]
    pub parent: Option<BitHash>,
    pub tree: BitHash,
}
