use crate::error::BitResult;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_commit(&self, message: String) -> BitResult<()> {
        let parent = self.read_head()?.map(|r| r.resolve(self)).transpose()?;
        let tree = self.write_tree()?;
        let hash = self.commit_tree(parent, message, tree)?;
        Ok(())
    }
}
