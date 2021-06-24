use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        let hash = self.write_tree()?;
        println!("{}", hash);
        Ok(())
    }

    /// create a tree from the index
    pub fn write_tree(&self) -> BitResult<Oid> {
        self.with_index(|index| index.write_tree())
    }
}
