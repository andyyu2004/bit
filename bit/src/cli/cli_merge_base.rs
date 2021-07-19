use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;

#[derive(Clap, Debug)]
pub struct BitMergeBaseCliOpts {
    a: LazyRevspec,
    b: LazyRevspec,
}

impl Cmd for BitMergeBaseCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let merge_base_commit = repo.merge_base(&self.a, &self.b)?;
        println!("{}", merge_base_commit.oid());
        Ok(())
    }
}
