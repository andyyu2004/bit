use clap::Clap;
use git_url_parse::GitUrl;
use libbit::error::BitResult;
use libbit::remote::DEFAULT_REMOTE;
use libbit::repo::BitRepo;
use std::path::{Path, PathBuf};

#[derive(Clap, Debug)]
pub struct BitCloneCliOpts {
    /// The repository to clone from
    url: String,
    /// The directory to clone into. If the directory exists it must be empty
    directory: Option<PathBuf>,
}

impl BitCloneCliOpts {
    pub fn exec(self, base_path: &Path) -> BitResult<()> {
        let url = GitUrl::parse(&self.url)?;
        let directory =
            base_path.join(self.directory.as_deref().unwrap_or_else(|| Path::new(&url.name)));
        eprintln!("cloning into `{}`", directory.display());
        if directory.exists() {
            ensure!(
                directory.read_dir()?.next().is_none(),
                "cannot clone into non-empty directory"
            );
        } else {
            std::fs::create_dir(&directory)?;
        }
        BitRepo::init_load(&directory, |repo| repo.add_remote(DEFAULT_REMOTE, &self.url))?;
        BitRepo::find(&directory, clone_async)
    }
}

#[tokio::main]
async fn clone_async(repo: BitRepo<'_>) -> BitResult<()> {
    repo.fetch(DEFAULT_REMOTE).await?;
    Ok(())
}
