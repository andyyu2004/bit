use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_diff_two_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let head_tree_iter = repo.head_tree_iter()?;
        Ok(())
    })
}
