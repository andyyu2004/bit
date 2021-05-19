use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_diff_two_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let head_tree_iter = repo.head_tree_iter()?;
        let oid = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let tree = repo.read_obj(oid)?.into_commit();
        Ok(())
    })
}
