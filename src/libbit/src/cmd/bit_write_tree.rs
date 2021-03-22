use crate::error::BitResult;
use crate::hash::BitHash;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        let hash = self.write_tree()?;
        println!("{}", hash);
        Ok(())
    }

    pub fn write_tree(&self) -> BitResult<BitHash> {
        let tree = self.with_index(|index| index.build_tree(self))?;
        let hash = self.write_obj(&tree)?;
        Ok(hash)
    }
}
