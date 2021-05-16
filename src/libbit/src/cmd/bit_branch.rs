use crate::error::BitResult;
use crate::refs::{self, BitRef};
use crate::repo::BitRepo;
use std::io::Write;

impl BitRepo {
    pub fn create_ref(&self, name: &str) -> BitResult<BitRef> {
        ensure!(refs::is_valid_name(name), "invalid branch name `{}`", name);
        let oid = self.resolve_head()?.try_into_oid()?;
        let mut file = self.mk_bitfile(format!("refs/heads/{}", name))?;
        file.write_all(oid.as_bytes())?;
        Ok(BitRef::Direct(oid.into()))
    }
}
