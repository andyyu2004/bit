use super::*;
use crate::pathspec::Pathspec;

#[test]
fn test_empty_tree_iter_yields_root_only() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let mut iter = repo.tree_iter(Oid::UNKNOWN);
        let root = iter.next()?.unwrap();
        assert_eq!(root.oid, Oid::UNKNOWN);
        assert_eq!(root.mode, FileMode::TREE);
        assert_eq!(root.path, BitPath::EMPTY);
        assert!(iter.next()?.is_none());
        Ok(())
    })
}
#[test]
fn test_tree_iterator_step_over() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::TREE);
        check_next!(iter.over() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_peekable() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.peek() => "":FileMode::TREE);
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.peek() => "bar":FileMode::REG);
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::TREE);
        check_next!(iter.next() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        assert_eq!(iter.peek()?, None);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_peekable_step_over_peeked() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.peek() => "bar":FileMode::REG);
        // remembers peeked value
        check_next!(iter.over() => "bar": FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::TREE);
        check_next!(iter.over() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_step_over_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        check_next!(iter.over() => "":FileMode::TREE);
        assert!(iter.next()?.is_none());
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_step_over() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.over() => "dir":FileMode::TREE);
        check_next!(iter.over() => "dir2":FileMode::TREE);
        check_next!(iter.over() => "exec":FileMode::EXEC);
        check_next!(iter.over() => "test.txt":FileMode::REG);
        check_next!(iter.over() => "zs":FileMode::TREE);
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_peek() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        check_next!(iter.peek() => "":FileMode::TREE);
        check_next!(iter.peek() => "":FileMode::TREE);
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.peek() => "dir":FileMode::TREE);
        check_next!(iter.next() => "dir":FileMode::TREE);
        check_next!(iter.next() => "dir/test.txt":FileMode::REG);
        check_next!(iter.peek() => "dir2":FileMode::TREE);
        check_next!(iter.peek() => "dir2":FileMode::TREE);
        check_next!(iter.next() => "dir2":FileMode::TREE);
        check_next!(iter.next() => "dir2/dir2.txt":FileMode::REG);
        check_next!(iter.next() => "dir2/nested":FileMode::TREE);
        check_next!(iter.next() => "dir2/nested/coolfile.txt":FileMode::REG);
        check_next!(iter.next() => "exec":FileMode::EXEC);
        check_next!(iter.next() => "test.txt":FileMode::REG);
        check_next!(iter.next() => "zs":FileMode::TREE);
        check_next!(iter.next() => "zs/one.txt":FileMode::REG);
        assert_eq!(iter.peek()?, None);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_collect_over_non_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        // step over root, "dir", and "dir/test.txt"
        iter.nth(2)?;
        let mut vec = vec![];
        iter.collect_over_tree_blobs(&mut vec)?;
        let paths = vec.iter().map(BitEntry::path).collect::<Vec<_>>();
        assert_eq!(paths, vec!["dir2/dir2.txt", "dir2/nested/coolfile.txt",]);
        Ok(())
    })
}

#[test]
fn test_tree_tree_iterator_step_over_multiple_nested() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let oid = tree! {
            outer {
                a {
                    x
                    y
                    z
                }
                b {
                    c {
                        k
                        m
                        n
                    }
                    d
                }
                c {
                    w
                    y
                }
            }
        };

        let mut iter = repo.tree_iter(oid);
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.next() => "outer":FileMode::TREE);
        check_next!(iter.over() => "outer/a":FileMode::TREE);
        check_next!(iter.next() => "outer/b":FileMode::TREE);
        check_next!(iter.over() => "outer/b/c":FileMode::TREE);
        check_next!(iter.over() => "outer/b/d":FileMode::REG);
        check_next!(iter.over() => "outer/c":FileMode::TREE);
        Ok(())
    })
}

#[test]
fn test_tree_tree_iterator_step_over_multiple() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        // this tree should match the directory structure below
        let oid = tree! {
            "dir0" {
                "test.txt"
            }
            "dir1" {
                "dir2.txt"
                "nested" {
                    "coolfile.txt"
                }
            }
            "dir2" {
            }
            "dir3" {
                "file"
            }
        };
        // this only tests `next` and not `peek` or `over`
        // we only compare paths as comparing modes is a bit pointless, and the the index may correctly have unknown oids
        let mut iter = repo.tree_iter(oid);
        check_next!(iter.next() => "":FileMode::TREE);
        check_next!(iter.over() => "dir0":FileMode::TREE);
        check_next!(iter.over() => "dir1":FileMode::TREE);
        check_next!(iter.over() => "dir2":FileMode::TREE);
        check_next!(iter.over() => "dir3":FileMode::TREE);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_collect_over_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        let mut vec = vec![];
        iter.collect_over_tree_blobs(&mut vec)?;
        let paths = vec.iter().map(BitEntry::path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "dir/test.txt",
                "dir2/dir2.txt",
                "dir2/nested/coolfile.txt",
                "exec",
                "test.txt",
                "zs/one.txt",
            ]
        );
        Ok(())
    })
}

#[test]
fn test_tree_tree_iterator_matches_index_tree_iterator() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        // this tree should match the directory structure below
        let oid = tree! {
            "dir" {
                "test.txt"
            }
            "dir2" {
                "dir2.txt"
                "nested" {
                    "coolfile.txt"
                }
            }
            "exec"
            "test.txt"
            "zs" {
                "one.txt"
            }
        };
        // this only tests `next` and not `peek` or `over`
        // we only compare paths as comparing modes is a bit pointless, and the the index may correctly have unknown oids
        let tree_tree_iter = repo.tree_iter(oid).map(|entry| Ok(entry.path()));
        let index_tree_iter = index.index_tree_iter().map(|entry| Ok(entry.path()));
        assert!(tree_tree_iter.eq(index_tree_iter)?);
        Ok(())
    })
}

/// ├── dir
/// │  └── test.txt
/// ├── dir2
/// │  ├── dir2.txt
/// │  └── nested
/// │     └── coolfile.txt
/// ├── exec
/// ├── test.txt
/// └── zs
///    └── one.txt
#[test]
fn test_index_tree_iterator_next() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter();
        check_next!(iter.next() => "": FileMode::TREE);
        check_next!(iter.next() => "dir": FileMode::TREE);
        check_next!(iter.next() => "dir/test.txt": FileMode::REG);
        check_next!(iter.next() => "dir2": FileMode::TREE);
        check_next!(iter.next() => "dir2/dir2.txt": FileMode::REG);
        check_next!(iter.next() => "dir2/nested": FileMode::TREE);
        check_next!(iter.next() => "dir2/nested/coolfile.txt": FileMode::REG);
        check_next!(iter.next() => "exec": FileMode::EXEC);
        check_next!(iter.next() => "test.txt": FileMode::REG);
        check_next!(iter.next() => "zs": FileMode::TREE);
        check_next!(iter.next() => "zs/one.txt": FileMode::REG);
        Ok(())
    })
}
// libbit/tests/repos/logic/logic-ir
// ├── Cargo.toml
// └── src
//    ├── ast_lowering
//    │  └── mod.rs
//    ├── debug.rs
//    ├── interned.rs
//    ├── interner.rs
//    ├── lib.rs
//    ├── tls.rs
//    └── unify.rs
#[test]
fn test_index_tree_iterator_on_logic_repo_index() -> BitResult<()> {
    // in particular, this tests when there are multilevel jumps in the index
    // if we look at the index, it contains the following entries
    // logic-ir/Cargo.toml,
    // logic-ir/src/ast_lowering/mod.rs,
    // logic-ir/src/debug.rs,
    //
    // we must yield pseudotree `logic-ir/src` before `logic-ir/ast_lowering`
    BitRepo::find(repos_dir!("logic"), |repo| {
        let index = repo.index()?;
        // we just look inside the logic-ir directory to make this test more manageable
        let pathspec = "logic-ir".parse::<Pathspec>()?;
        let mut iter = pathspec.match_tree_iter(index.index_tree_iter());
        dbg!(index.entries().keys().map(|(entry, _)| entry).collect::<Vec<_>>());
        check_next!(iter.next() => "logic-ir":FileMode::TREE);
        check_next!(iter.next() => "logic-ir/Cargo.toml":FileMode::REG);
        check_next!(iter.next() => "logic-ir/src":FileMode::TREE);
        check_next!(iter.next() => "logic-ir/src/ast_lowering":FileMode::TREE);
        check_next!(iter.next() => "logic-ir/src/ast_lowering/mod.rs":FileMode::REG);
        check_next!(iter.next() => "logic-ir/src/debug.rs":FileMode::REG);
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_filter() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        let index = repo.index()?;
        let mut iter = index.index_tree_iter().filter(|entry| Ok(entry.path().starts_with("dir2")));
        check_next!(iter.peek() => "dir2": FileMode::TREE);
        check_next!(iter.next() => "dir2": FileMode::TREE);
        check_next!(iter.next() => "dir2/dir2.txt": FileMode::REG);
        check_next!(iter.peek() => "dir2/nested": FileMode::TREE);
        check_next!(iter.over() => "dir2/nested": FileMode::TREE);
        assert!(iter.next()?.is_none());
        Ok(())
    })
}

#[test]
fn test_build_tree_from_tree_iter() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut tree_iter = repo.head_tree_iter()?;
        let tree = tree_iter.build_tree(repo, None)?;
        assert_eq!(tree, repo.head_tree()?);
        Ok(())
    })
}
