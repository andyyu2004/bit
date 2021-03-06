use super::BitTreeCache;
use crate::error::BitResult;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use indexmap::indexmap;
use std::lazy::SyncLazy;

// this breaks if we change const to static
const CACHE_TREE: SyncLazy<BitTreeCache> = SyncLazy::new(|| BitTreeCache {
    path: BitPath::EMPTY,
    entry_count: 5,
    children: indexmap! {
        "dir".into() => BitTreeCache {
            path: "dir".into(),
            entry_count: 3,
            children: indexmap! {
                "bar".into() => CACHE_TREE_DIR_BAR.clone()
            },
            tree_oid: "9ffa74fdebe76f339dfc5d40a63ddf9d0cba4b06".into(),
        }
    },
    tree_oid: "f3560f770ad0986e851d302b1d400588d5792f67".into(),
});

const CACHE_TREE_DIR_BAR: SyncLazy<BitTreeCache> = SyncLazy::new(|| BitTreeCache {
    path: "bar".into(),
    entry_count: 1,
    children: indexmap! {},
    tree_oid: "29ba47b07d262ad717095f2d94ec771194c4c083".into(),
});

#[test]
fn test_read_tree_cache_from_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let head_tree = repo.head_tree()?;
        let tree_cache = BitTreeCache::read_tree(repo, head_tree)?;
        assert_eq!(tree_cache, *CACHE_TREE);
        Ok(())
    })
}

#[test]
fn test_tree_cache_find_child() {
    let cache_tree = &*CACHE_TREE;
    let child = cache_tree.find_child("dir/bar".into()).unwrap();
    assert_eq!(child, &*CACHE_TREE_DIR_BAR);
}

#[test]
fn test_tree_cache_invalidate_path() {
    let mut cache_tree = CACHE_TREE.clone();
    cache_tree.invalidate_path("dir".into());

    let mut expected_tree = CACHE_TREE.clone();
    expected_tree.invalidate();
    expected_tree.find_child_mut("dir".into()).unwrap().invalidate();

    assert_eq!(cache_tree, expected_tree);
}

#[test]
fn test_tree_cache_find_valid_child() {
    let cache = BitTreeCache {
        path: BitPath::EMPTY,
        entry_count: -1,
        children: indexmap! {
            "zs".into() => BitTreeCache {
                path: "zs".into(),
                entry_count: 1,
                children: indexmap! {},
                tree_oid: "2d7f016d4251e5ad28c1a88bf34e849f33fc772c".into(),
            },
            "dir".into() => BitTreeCache {
                path: "dir".into(),
                entry_count: 1,
                children: indexmap! {},
                tree_oid: "920512d27e4df0c79ca4a929bc5d4254b3d05c4c".into(),
            },
            "dir2".into() => BitTreeCache {
                path: "dir2".into(),
                entry_count: 2,
                children: indexmap! {
                    "nested".into() => BitTreeCache {
                        path: "nested".into(),
                        entry_count: 1,
                        children: indexmap! {},
                        tree_oid: "922a85d55bd55028593c9816724c874c5629b557".into(),
                    }
                },
                tree_oid: "fa9b4c62cce3b8a2d60482d717026beb46b1245c".into(),
            },
        },
        tree_oid: "0000000000000000000000000000000000000000".into(),
    };
    assert!(cache.find_valid_child("zs".into()).is_some());
}

const EXPECTED_POST_UPDATE_TREE: SyncLazy<BitTreeCache> = SyncLazy::new(|| BitTreeCache {
    path: BitPath::EMPTY,
    tree_oid: "88512acb9a9a07ae8d7eb0128c28384840db6148".into(),
    entry_count: 3,
    children: indexmap! {
        p!("foo") => BitTreeCache {
            path: p!("foo"),
            tree_oid: "bef764b817d90289a189126664cdad61a28b1fbb".into(),
            entry_count: 2,
            children: indexmap!{
                p!("baz") => BitTreeCache {
                    path: p!("baz"),
                    tree_oid: "2a26db49a6962700da5bd4084ae0e5a22d6583ee".into(),
                    entry_count: 1,
                    children: indexmap!{},
                },
            },
        },
        p!("unchanged") => BitTreeCache {
            path: p!("unchanged"),
            tree_oid: "c0164c1f7de74195e6c5787976f97f1c2102d13d".into(),
            entry_count: 1,
            children: indexmap!{
                p!("x") => BitTreeCache {
                    path: p!("x"),
                    tree_oid: "409cc707b732a2a5af3939d0eab200cbc93ed065".into(),
                    entry_count: 1,
                    children: indexmap! {
                        p!("y") => BitTreeCache {
                            path: p!("y"),
                            tree_oid: "08c56681eceec443b14ad503fa7ebf1c46652c50".into(),
                            entry_count: 1,
                            children: indexmap!{},
                        },
                    },
                },
            },
        },
    },
});

// used for the following two tests
// must be called from within a repo context
fn mk_modified_tree() -> Oid {
    tree! {
        foo {
            a
            baz {
                d
            }
        }
        unchanged {
            x {
               y {
                   z
               }
            }
        }
    }
}

#[test]
fn test_tree_cache_update() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let initial_tree = tree! {
            foo {
                a
                bar {
                    b
                }
            }
            unchanged {
                x {
                   y {
                       z
                   }
                }
            }
        };

        let modified_tree = mk_modified_tree();
        let mut tree_cache = BitTreeCache::read_tree(repo, initial_tree)?;
        tree_cache.update(repo, modified_tree)?;

        assert_eq!(tree_cache, *EXPECTED_POST_UPDATE_TREE);
        Ok(())
    })
}

#[test]
fn test_tree_cache_update_with_invalidated_children() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let initial_tree = tree! {
            unchanged {
                x {
                   y {
                       z
                   }
                }
            }
        };

        let modified_tree = mk_modified_tree();

        let mut tree_cache = BitTreeCache::read_tree(repo, initial_tree)?;
        tree_cache.invalidate_path(p!("unchanged/x"));
        tree_cache.update(repo, modified_tree)?;
        assert_eq!(tree_cache, *EXPECTED_POST_UPDATE_TREE);
        Ok(())
    })
}
