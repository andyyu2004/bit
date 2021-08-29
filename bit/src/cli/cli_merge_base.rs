use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Clap, Debug)]
pub struct BitMergeBaseCliOpts {
    a: Revspec,
    b: Revspec,
}

impl Cmd for BitMergeBaseCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let a = repo.fully_resolve_rev(&self.a)?;
        let b = repo.fully_resolve_rev(&self.b)?;
        match repo.merge_base(a, b)? {
            Some(merge_base) => println!("{}", merge_base.oid()),
            None => println!("no merge base found"),
        }
        Ok(())
    }
}
