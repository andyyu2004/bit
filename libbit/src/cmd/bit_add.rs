use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

impl BitRepo {
    pub fn bit_add_dryrun(&self, pathspecs: &[Pathspec]) -> BitResult<()> {
        let index = self.index()?;
        for pathspec in pathspecs {
            pathspec
                .match_worktree(&index)?
                .for_each(|entry| Ok(println!("add `{}`", entry.path)))?;
        }
        Ok(())
    }

    pub fn bit_add_all(&self) -> BitResult<()> {
        self.index_mut()?.add_all()
    }

    pub fn bit_add(&self, pathspecs: &[Pathspec]) -> BitResult<()> {
        let mut index = self.index_mut()?;
        for pathspec in pathspecs {
            index.add(pathspec)?;
        }
        Ok(())
    }
}
