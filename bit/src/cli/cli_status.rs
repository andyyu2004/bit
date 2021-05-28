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
        let status = match self.pathspec {
            Some(pathspec) => repo.scoped_status_report(pathspec),
            None => repo.status_report(),
        }?;
        Ok(print!("{}", status))
    }
}
