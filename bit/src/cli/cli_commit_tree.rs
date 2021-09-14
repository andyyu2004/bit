use clap::Clap;
use libbit::obj::Oid;

#[derive(Clap, Debug)]
pub struct BitCommitTreeCliOpts {
    #[clap(short = 'm', long = "message")]
    pub message: Option<String>,
    #[clap(short = 'p', long = "parent")]
    pub parents: Vec<Oid>,
    pub tree: Oid,
}
