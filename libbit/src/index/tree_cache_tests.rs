use std::lazy::Lazy;

use super::BitTreeCache;
use crate::error::BitResult;
use crate::path::BitPath;
use crate::repo::BitRepo;

const CACHE_TREE: Lazy<BitTreeCache> = Lazy::new(|| BitTreeCache {
    path: BitPath::EMPTY,
    entry_count: 5,
    children: vec![BitTreeCache {
        path: "dir".into(),
        entry_count: 3,
        children: vec![CACHE_TREE_DIR_BAR.clone()],
        oid: "9ffa74fdebe76f339dfc5d40a63ddf9d0cba4b06".into(),
    }],
    oid: "f3560f770ad0986e851d302b1d400588d5792f67".into(),
});

const CACHE_TREE_DIR_BAR: Lazy<BitTreeCache> = Lazy::new(|| BitTreeCache {
    path: "dir/bar".into(),
    entry_count: 1,
    children: vec![],
    oid: "29ba47b07d262ad717095f2d94ec771194c4c083".into(),
});

#[test]
fn test_read_tree_cache_from_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let head_tree = repo.head_tree()?;
        let tree_cache = BitTreeCache::read_tree_cache(repo, &head_tree)?;
        assert_eq!(tree_cache, *CACHE_TREE);
        Ok(())
    })
}

#[test]
fn test_tree_cache_find_child() {
    let cache_tree = &*CACHE_TREE;
    let child = cache_tree.find_child("dir/bar").unwrap();
    assert_eq!(child, &*CACHE_TREE_DIR_BAR);
}

#[test]
fn test_tree_cache_invalidate_path() {
    let mut cache_tree = CACHE_TREE.clone();
    cache_tree.invalidate_path("dir".into());

    let mut expected_tree = CACHE_TREE.clone();
    expected_tree.entry_count = -1;
    expected_tree.find_child_mut("dir").unwrap().entry_count = -1;

    assert_eq!(cache_tree, expected_tree);
}
