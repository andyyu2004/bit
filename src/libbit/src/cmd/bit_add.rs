use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

#[derive(Debug)]
pub struct BitAddOpts {
    pub pathspecs: Vec<Pathspec>,
}

impl BitRepo {
    pub fn bit_add(&self, opts: BitAddOpts) -> BitResult<()> {
        // tls::with_index_mut(|index| index.add_all(&opts.pathspecs))?;
        for pathspec in opts.pathspecs {
            pathspec.match_worktree().for_each(|_pathspec_match| {
                // dbg!(pathspec_match);
                Ok(())
            })?;
        }
        Ok(())
    }
}
