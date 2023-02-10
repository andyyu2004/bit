use super::Cmd;
use clap::Parser;
use libbit::commit::CommitOpts;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

#[derive(Parser, Debug)]
pub struct BitCommitCliOpts {
    #[arg(short = 'm', long = "message")]
    pub message: Option<String>,
    #[arg(long = "allow-empty")]
    pub allow_empty: bool,
}

impl Cmd for BitCommitCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
        let opts = CommitOpts { message: self.message, allow_empty: self.allow_empty };
        let summary = repo.commit(opts)?;
        print!("{summary}");
        Ok(())
    }
}
