use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::repo::BitRepo;

#[derive(Parser, Debug)]
pub struct BitFetchCliOpts {
    remote: Option<String>,
}

impl Cmd for BitFetchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        self.exec_async(repo)
    }
}

impl BitFetchCliOpts {
    #[tokio::main]
    async fn exec_async(self, repo: BitRepo<'_>) -> BitResult<()> {
        match self.remote {
            Some(remote) => {
                repo.fetch(&remote).await?;
            }
            None => {
                // TODO run these using join concurrently
                for remote in repo.ls_remotes() {
                    repo.fetch(remote.name).await?;
                }
            }
        };
        Ok(())
    }
}
