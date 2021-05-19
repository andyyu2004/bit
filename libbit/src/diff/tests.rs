use crate::error::BitResult;
use crate::obj::Treeish;
use crate::repo::BitRepo;

#[test]
fn test_diff_two_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let oid = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let tree = repo.read_obj(oid)?.into_commit().into_tree()?;
        let head_tree = repo.head_tree()?;
        let diff = repo.diff_tree_to_tree(&tree, &head_tree)?;
        Ok(())
    })
}
