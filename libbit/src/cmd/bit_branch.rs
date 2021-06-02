use crate::error::BitResult;
use crate::refs::{self, SymbolicRef};
use crate::repo::BitRepo;

impl<'r> BitRepo<'r> {
    pub fn bit_create_branch(&self, name: &str) -> BitResult<()> {
        ensure!(refs::is_valid_name(name), "invalid branch name `{}`", name);
        self.create_branch(SymbolicRef::branch(name), SymbolicRef::HEAD)?;
        Ok(())
    }
}
