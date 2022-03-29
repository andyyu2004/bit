use clap::Parser;
use git_url_parse::GitUrl;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
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
        BitRepo::clone_blocking(directory, &self.url)
    }
}
