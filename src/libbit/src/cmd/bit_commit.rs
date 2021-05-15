use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    pub fn commit(&self, message: Option<String>) -> BitResult<Oid> {
        let parent = self.resolved_head()?;
        let tree = self.write_tree()?;
        let oid = self.commit_tree(parent, message, tree)?;
        self.update_head(oid)?;
        Ok(oid)
    }
}
