use crate::error::BitResult;
use crate::obj::{Commit, Oid, Tree};
use crate::repo::BitRepo;

// experimental
pub trait Peel {
    type Peeled;
    fn peel(&self, repo: BitRepo<'_>) -> BitResult<Self::Peeled>;
}

// peeling oid into a commit makes more sense than peeling into a tree
// as we can just use treeish for that
// furthermore, we often want the tree oid given an commit_oid
// however, this is sort of subtle/arbitrary and probably not great design
impl Peel for Oid {
    type Peeled = Commit;

    fn peel(&self, repo: BitRepo<'_>) -> BitResult<Self::Peeled> {
        Ok(repo.read_obj(*self)?.into_commit())
    }
}

impl Peel for Commit {
    type Peeled = Tree;

    fn peel(&self, repo: BitRepo<'_>) -> BitResult<Self::Peeled> {
        repo.read_obj(self.tree)?.into_tree()
    }
}