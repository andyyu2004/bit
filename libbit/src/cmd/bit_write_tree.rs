use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl<'r> BitRepo<'r> {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        let hash = self.write_tree()?;
        println!("{}", hash);
        Ok(())
    }

    /// builds a tree object from the index and writes it to the object store
    pub fn write_tree(&self) -> BitResult<Oid> {
        let tree = self.with_index(|index| index.build_tree())?;
        let oid = self.write_obj(&tree)?;
        Ok(oid)
    }
}
