use clap::Clap;
use libbit::hash::BitHash;

#[derive(Clap, Debug)]
pub struct BitCommitTreeCliOpts {
    #[clap(short = 'm')]
    pub message: Option<String>,
    #[clap(short = 'p')]
    pub parent: Option<BitHash>,
    pub tree: BitHash,
}
