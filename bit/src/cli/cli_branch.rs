use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitBranchCliOpts {
    name: String,
    #[clap(default_value = "HEAD")]
    revision: Revspec,
}

impl Cmd for BitBranchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        match repo.try_fully_resolve_rev(&self.revision)? {
            Some(..) => repo.bit_create_branch(&self.name, &self.revision),
            // we can't actually create a new branch on an `empty branch`
            // as the branch doesn't actually exist yet.
            // all that exists is the reference to it in HEAD.
            // all sorts of edge cases come up on an empty repos unfortunately
            None => bail!(
                "cannot create new branch in an empty repository (use `bit switch -c <branch>` to change your branch)"
            ),
        }?;
        Ok(())
    }
}
