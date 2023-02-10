use clap::Parser;
use libbit::obj::Oid;

#[derive(Parser, Debug)]
pub struct BitCommitTreeCliOpts {
    #[arg(short = 'm', long = "message")]
    pub message: Option<String>,
    #[arg(short = 'p', long = "parent")]
    pub parents: Vec<Oid>,
    pub tree: Oid,
}
