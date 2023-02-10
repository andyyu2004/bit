use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::iter::FallibleIterator;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Parser, Debug)]
pub struct BitRevlistCliOpts {
    #[clap(required = true)]
    revisions: Vec<Revspec>,
}

impl Cmd for BitRevlistCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
        let revisions = self.revisions.iter().collect::<Vec<_>>();
        let revwalk = repo.revwalk(&revisions)?;
        revwalk.for_each(|commit| {
            println!("{}", commit.oid());
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_revlist_requires_at_least_one_revision() {
        assert!(BitRevlistCliOpts::try_parse_from(["--"]).is_err());
    }

    #[test]
    fn test_parse_revlist_cli() {
        let opts = BitRevlistCliOpts::try_parse_from(["--", "HEAD", "master", "branch"]).unwrap();
        assert_eq!(opts.revisions.len(), 3);
    }
}
