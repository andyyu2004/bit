use crate::error::BitResult;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_write_tree(&self) -> BitResult<()> {
        self.with_index(|index| {
            let _tree = index.write_tree(self)?;
            dbg!(_tree);
            Ok(())
        })
    }
}
