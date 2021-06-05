use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl<'r> BitRepo<'r> {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        let hash = self.write_tree()?;
        println!("{}", hash);
        Ok(())
    }

    pub fn write_tree(&self) -> BitResult<Oid> {
        let tree = self.with_index(|index| index.write_tree())?;
        let oid = self.write_obj(&tree)?;
        Ok(oid)
    }
}
