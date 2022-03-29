use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Parser, Debug)]
pub struct BitCherryPickCliOpts {
    revisions: Vec<Revspec>,
}

impl Cmd for BitCherryPickCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let refs = self
            .revisions
            .iter()
            .map(|rev| repo.resolve_rev(rev))
            .collect::<Result<Vec<_>, _>>()?;
        repo.cherrypick_many(refs)?;
        Ok(())
    }
}
