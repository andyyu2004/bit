use crate::error::BitResult;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_commit(&self, message: String) -> BitResult<()> {
        let hash = self.commit_tree(parent, message, tree)?;
        println!("{}", hash);
        Ok(())
    }
}
