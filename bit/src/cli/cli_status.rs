use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::pathspec::Pathspec;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitStatusCliOpts {
    pathspec: Option<Pathspec>,
}

impl Cmd for BitStatusCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        let pathspec = self.pathspec.unwrap_or(Pathspec::MATCH_ALL);
        let status = repo.status(pathspec)?;
        Ok(print!("{}", status))
    }
}
