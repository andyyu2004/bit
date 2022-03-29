use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::merge::{MergeOpts, MergeResults};
use libbit::repo::BitRepo;
use libbit::rev::Revspec;
use libbit::xdiff::DiffFormatExt;

#[derive(Parser, Debug)]
pub struct BitMergeCliOpts {
    revision: Revspec,
    #[clap(long = "no-commit")]
    no_commit: bool,
    #[clap(long = "no-edit")]
    no_edit: bool,
    #[clap(long = "no-ff")]
    no_ff: bool,
}

impl Cmd for BitMergeCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
        let mut opts = MergeOpts::default();
        opts.no_commit = self.no_commit;
        opts.no_edit = self.no_edit;
        opts.no_ff = self.no_ff;

        match repo.merge_rev(&self.revision, opts)? {
            MergeResults::Null => println!("already up to date"),
            MergeResults::FastForward { from, to } => {
                // Updating f160da1..7e6f94d
                // Fast-forward
                //  foo | 0
                //  1 file changed, 0 insertions(+), 0 deletions(-)
                //  create mode 100644 foo
                println!("Updating {}..{}", from.short(), to.short());
                println!("Fast-forward");
                let diff = repo.diff_tree_to_tree(from, to)?;
                diff.print_diffstat(repo)?;
                diff.print_change_summary()?;
            }
            MergeResults::Merge(_summary) => println!("todo merge message"),
            MergeResults::Conflicts(_) => println!("todo print merge conflicts"),
        }
        Ok(())
    }
}
