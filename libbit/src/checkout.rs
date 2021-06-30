use crate::error::BitResult;
use crate::obj::Oid;
use crate::repo::BitRepo;

impl<'rcx> BitRepo<'rcx> {
    /// update the worktree to match the tree represented by `target`
    pub fn checkout_tree(&self, target_tree: Oid) -> BitResult<()> {
        let baseline = self.head_tree_iter()?;
        let target = self.tree_iter(target_tree);
        // let workdir = self.with_index(|index| index.worktree_iter())?;
        Ok(())
    }
}
