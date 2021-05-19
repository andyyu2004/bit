use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

impl BitRepo {
    pub fn bit_add_dryrun(&self, pathspecs: &[Pathspec]) -> BitResult<()> {
        for pathspec in pathspecs {
            self.match_worktree_with(pathspec)?
                .for_each(|entry| Ok(println!("add `{}`", entry.filepath)))?;
        }
        Ok(())
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
