use crate::error::BitResult;
use crate::hash::BitHash;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn commit_tree(
        &self,
        parent: Option<BitHash>,
        message: String,
        tree: BitHash,
    ) -> BitResult<()> {
        self.mk_commit(tree, message, parent)?;
        Ok(())
    }
}
