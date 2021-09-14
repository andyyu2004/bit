use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::merge::{MergeOpts, MergeResults};
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitMergeCliOpts {
    revision: Revspec,
    #[clap(long = "no-commit")]
    no_commit: bool,
}

impl Cmd for BitMergeCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let mut opts = MergeOpts::default();
        opts.no_commit = self.no_commit;

        match repo.merge_rev(&self.revision, opts)? {
            // Updating f160da1..7e6f94d
            // Fast-forward
            //  foo | 0
            //  1 file changed, 0 insertions(+), 0 deletions(-)
            //  create mode 100644 foo
            MergeResults::Null => println!("already up to date"),
            MergeResults::FastForward { to } => println!("todo some ff message `{}`", to),
            MergeResults::Merge(_) => println!("todo merge message"),
        }
        Ok(())
    }
}
