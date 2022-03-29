use clap::Parser;
use libbit::error::BitResult;
use libbit::repo::{BitRepo, InitSummary};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
pub struct BitInitCliOpts {
    #[clap(default_value = ".")]
    pub path: PathBuf,
}

impl BitInitCliOpts {
    pub fn exec(self, base_path: &Path) -> BitResult<()> {
        let path = base_path.join(&self.path);
        match BitRepo::init(&self.path)? {
            InitSummary::Init =>
                println!("initialized empty bit repository in `{}`", path.display()),
            InitSummary::Reinit =>
                println!("reinitialized existing bit repository in `{}`", path.display()),
        }
        Ok(())
    }
}
