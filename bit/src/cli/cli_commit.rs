use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

use super::Cmd;

#[derive(Clap, Debug)]
pub struct BitCommitCliOpts {
    #[clap(short = 'm', long = "message")]
    pub message: Option<String>,
}

impl Cmd for BitCommitCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let summary = repo.commit(self.message)?;
        print!("{}", summary);
        Ok(())
    }
}
