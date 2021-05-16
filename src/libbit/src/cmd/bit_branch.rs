use crate::error::BitResult;
use crate::refs::{self, SymbolicRef};
use crate::repo::BitRepo;

impl BitRepo {
    pub fn create_branch(&self, name: &str) -> BitResult<()> {
        ensure!(refs::is_valid_name(name), "invalid branch name `{}`", name);
        self.create_ref(SymbolicRef::branch(name), self.head_ref())?;
        Ok(())
    }
}
