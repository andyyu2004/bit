use clap::Clap;

#[derive(Clap, Debug)]
pub struct BitCommitCliOpts {
    #[clap(short = 'm', long = "message")]
    pub message: Option<String>,
}
