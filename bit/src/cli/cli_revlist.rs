use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitRevlistCliOpts {
    // TODO require at least one revision
    revisions: Vec<LazyRevspec>,
}

impl Cmd for BitRevlistCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let revisions = self.revisions.iter().collect::<Vec<_>>();
        let revlist = repo.revlist(&revisions)?;
        print!("{}", revlist);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_revlist_cli() -> BitResult<()> {
        let opts = BitRevlistCliOpts::parse_from(&["--", "HEAD", "master", "branch"]);
        assert_eq!(opts.revisions.len(), 3);
        Ok(())
    }
}
