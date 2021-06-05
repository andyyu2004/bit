use super::BitTreeCache;
use crate::error::BitResult;
use crate::path::BitPath;
use crate::repo::BitRepo;

#[test]
fn test_read_tree_cache_from_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let head_tree = repo.head_tree()?;
        let tree_cache = BitTreeCache::read_tree_cache(repo, &head_tree)?;
        let expected_tree_cache = BitTreeCache {
            path: BitPath::EMPTY,
            entry_count: 5,
            children: vec![BitTreeCache {
                path: "dir".into(),
                entry_count: 3,
                children: vec![BitTreeCache {
                    path: "dir/bar".into(),
                    entry_count: 1,
                    children: vec![],
                    oid: "29ba47b07d262ad717095f2d94ec771194c4c083".into(),
                }],
                oid: "9ffa74fdebe76f339dfc5d40a63ddf9d0cba4b06".into(),
            }],
            oid: "f3560f770ad0986e851d302b1d400588d5792f67".into(),
        };

        assert_eq!(tree_cache, expected_tree_cache);
        Ok(())
    })
}
