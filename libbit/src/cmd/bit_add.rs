use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_add_dryrun(&self, pathspecs: &[Pathspec]) -> BitResult<()> {
        self.with_index(|index| {
            for pathspec in pathspecs {
                pathspec
                    .match_worktree(index)?
                    .for_each(|entry| Ok(println!("add `{}`", entry.path)))?;
            }
            Ok(())
        })
    }

    pub fn bit_add_all(&self) -> BitResult<()> {
        self.with_index_mut(|index| index.add_all())
    }

    pub fn bit_add(&self, pathspecs: &[Pathspec]) -> BitResult<()> {
        self.with_index_mut(|index| {
            for pathspec in pathspecs {
                index.add(&pathspec)?;
            }
            Ok(())
        })
    }
}
