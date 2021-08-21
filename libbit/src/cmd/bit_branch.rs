use crate::error::BitResult;
use crate::refs::{self, SymbolicRef};
use crate::repo::BitRepo;
use crate::rev::Revspec;

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_create_branch(&self, name: &str, from: &Revspec) -> BitResult<SymbolicRef> {
        ensure!(refs::is_valid_name(name), "invalid branch name `{}`", name);
        let sym = SymbolicRef::new_branch(name);
        self.create_branch(sym, from)?;
        Ok(sym)
    }
}
