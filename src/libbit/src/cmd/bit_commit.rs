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
        let parent = self.read_head()?.map(|r| r.resolve(self)).transpose()?;
        let tree = self.write_tree()?;
        let hash = self.commit_tree(parent, message, tree)?;
        let bitref = BitRef::Direct(hash);
        self.update_head(bitref)?;
        Ok(bitref)
    }
}
