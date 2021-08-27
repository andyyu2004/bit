use crate::error::BitResult;
use crate::obj::{BitObject, Commit, Oid, Tree};
use crate::repo::BitRepo;

// experimental
pub trait Peel<'rcx> {
    type Peeled;
    fn peel(&self, repo: BitRepo<'rcx>) -> BitResult<Self::Peeled>;
}

// peeling oid into a commit makes more sense than peeling into a tree
// as we can just use treeish for that
// furthermore, we often want the tree oid given an commit_oid
// however, this is sort of subtle/arbitrary and probably not great design
impl<'rcx> Peel<'rcx> for Oid {
    type Peeled = Commit<'rcx>;

    fn peel(&self, repo: BitRepo<'rcx>) -> BitResult<Self::Peeled> {
        repo.read_obj(*self)?.try_into_commit()
    }
}

impl<'rcx> Peel<'rcx> for Commit<'rcx> {
    type Peeled = Tree<'rcx>;

    fn peel(&self, repo: BitRepo<'rcx>) -> BitResult<Self::Peeled> {
        debug_assert!(repo == self.owner());
        Ok(self.owner().read_obj_tree(self.tree)?)
    }
}
