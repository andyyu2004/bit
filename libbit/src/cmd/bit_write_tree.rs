use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        let hash = self.write_tree()?;
        println!("{hash}");
        Ok(())
    }

    /// create a tree from the index
    pub fn write_tree(&self) -> BitResult<Oid> {
        self.index_mut()?.write_tree()
    }
}
