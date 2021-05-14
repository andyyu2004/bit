use crate::error::BitResult;
use crate::refs::BitRef;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    pub fn commit(&self, message: Option<String>) -> BitResult<BitRef> {
        let parent = self.resolved_head()?;
        let tree = self.write_tree()?;
        let oid = self.commit_tree(parent, message, tree)?;
        let bitref = BitRef::Direct(oid.into());
        self.update_head(bitref)?;
        Ok(bitref)
    }
}
