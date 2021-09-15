use super::Cmd;
use clap::Clap;
use libbit::commit::CommitOpts;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitCommitCliOpts {
    #[clap(short = 'm', long = "message")]
    pub message: Option<String>,
    #[clap(long = "allow-empty")]
    pub allow_empty: bool,
}

impl Cmd for BitCommitCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let mut opts = CommitOpts::default();
        opts.message = self.message;
        opts.allow_empty = self.allow_empty;
        let summary = repo.commit(opts)?;
        print!("{}", summary);
        Ok(())
    }
}
