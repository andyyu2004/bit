use super::BitTreeCache;
use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_read_tree_cache_from_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        dbg!(repo.index_path());
        let head_tree = repo.head_tree()?;
        dbg!(&head_tree);
        let tree_cache = BitTreeCache::read_tree(repo, &head_tree)?;
        dbg!(tree_cache);
        Ok(())
    })
}
