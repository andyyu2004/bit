use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitRevlistCliOpts {
    #[clap(required = true)]
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
    fn test_parse_revlist_requires_at_least_one_revision() {
        assert!(BitRevlistCliOpts::try_parse_from(&["--"]).is_err());
    }

    #[test]
    fn test_parse_revlist_cli() -> BitResult<()> {
        let opts = BitRevlistCliOpts::try_parse_from(&["--", "HEAD", "master", "branch"])?;
        assert_eq!(opts.revisions.len(), 3);
        Ok(())
    }
}
