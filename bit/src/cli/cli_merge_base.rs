use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Parser, Debug)]
pub struct BitMergeBaseCliOpts {
    a: Revspec,
    b: Revspec,
    #[clap(short = 'a', long = "all")]
    all: bool,
}

impl Cmd for BitMergeBaseCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
        let a = repo.fully_resolve_rev(&self.a)?;
        let b = repo.fully_resolve_rev(&self.b)?;

        let merge_bases = if self.all {
            repo.merge_bases(a, b)?
        } else {
            repo.merge_base(a, b)?.into_iter().collect()
        };

        if merge_bases.is_empty() {
            println!("no merge base found")
        } else {
            for merge_base in merge_bases {
                println!("{}", merge_base.oid())
            }
        }
        Ok(())
    }
}
