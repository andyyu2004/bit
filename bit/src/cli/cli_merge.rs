use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::merge::MergeKind;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitMergeCliOpts {
    revision: Revspec,
}

impl Cmd for BitMergeCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        match repo.merge(&self.revision)? {
            // Updating f160da1..7e6f94d
            // Fast-forward
            //  foo | 0
            //  1 file changed, 0 insertions(+), 0 deletions(-)
            //  create mode 100644 foo
            MergeKind::FastForward => println!("idk some ff message"),
            MergeKind::Null => println!("already up to date"),
            MergeKind::Merge(_) => todo!(),
        }
        Ok(())
    }
}
