use crate::error::BitResult;
use crate::refs::{self, SymbolicRef};
use crate::repo::BitRepo;
use crate::rev::Revspec;

impl BitRepo {
    pub fn bit_create_branch(&self, name: &str, from: &Revspec) -> BitResult<SymbolicRef> {
        ensure!(refs::is_valid_name(name), "invalid branch name `{}`", name);
        let sym = SymbolicRef::new_branch(name);
        let reference = self.resolve_rev(from)?;
        self.create_branch(sym, reference)?;
        Ok(sym)
    }
}
