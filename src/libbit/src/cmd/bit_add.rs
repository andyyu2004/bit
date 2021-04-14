use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::FallibleIterator;

#[derive(Debug)]
pub struct BitAddOpts {
    pub pathspecs: Vec<Pathspec>,
    pub dryrun: bool,
}

impl BitRepo {
    pub fn bit_add(&self, opts: BitAddOpts) -> BitResult<()> {
        tls::with_index_mut(|index| {
            for pathspec in opts.pathspecs {
                if opts.dryrun {
                    pathspec
                        .match_worktree()?
                        .for_each(|entry| Ok(println!("add `{}`", entry.filepath)))?;
                } else {
                    index.add(&pathspec)?;
                }
            }
            Ok(())
        })
    }
}
