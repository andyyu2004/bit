use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::FallibleIterator;

#[derive(Debug)]
pub struct BitAddOpts {
    pub pathspecs: Vec<Pathspec>,
}

impl BitRepo {
    pub fn bit_add(&self, opts: BitAddOpts) -> BitResult<()> {
        // tls::with_index_mut(|index| index.add_all(&opts.pathspecs))?;
        for pathspec in opts.pathspecs {
            dbg!(pathspec);
            tls::with_repo(|repo| {
                while let Some(entry) = repo.worktree_iter().next()? {
                    dbg!(entry);
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
