use super::*;

#[test]
fn test_tree_iterator_step_over() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.next() => "":FileMode::DIR);
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::DIR);
        check_next!(iter.over() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_peekable() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.peek() => "":FileMode::DIR);
        check_next!(iter.next() => "":FileMode::DIR);
        check_next!(iter.peek() => "bar":FileMode::REG);
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::DIR);
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
        check_next!(iter.next() => "":FileMode::DIR);
        check_next!(iter.peek() => "bar":FileMode::REG);
        // remembers peeked value
        check_next!(iter.over() => "bar": FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::DIR);
        check_next!(iter.over() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_step_over_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            check_next!(iter.over() => "":FileMode::DIR);
            assert!(iter.next()?.is_none());
            Ok(())
        })
    })
}

#[test]
fn test_index_tree_iterator_step_over() -> BitResult<()> {
    // TODO can't actually run these concurrently on the same repo...
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            check_next!(iter.next() => "":FileMode::DIR);
            check_next!(iter.over() => "dir":FileMode::DIR);
            check_next!(iter.over() => "dir2":FileMode::DIR);
            check_next!(iter.over() => "exec":FileMode::EXEC);
            check_next!(iter.over() => "test.txt":FileMode::REG);
            check_next!(iter.over() => "zs":FileMode::DIR);
            Ok(())
        })
    })
}

#[test]
fn test_index_tree_iterator_peek() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            check_next!(iter.peek() => "":FileMode::DIR);
            check_next!(iter.peek() => "":FileMode::DIR);
            check_next!(iter.next() => "":FileMode::DIR);
            check_next!(iter.peek() => "dir":FileMode::DIR);
            check_next!(iter.next() => "dir":FileMode::DIR);
            check_next!(iter.next() => "dir/test.txt":FileMode::REG);
            check_next!(iter.peek() => "dir2":FileMode::DIR);
            check_next!(iter.peek() => "dir2":FileMode::DIR);
            check_next!(iter.next() => "dir2":FileMode::DIR);
            check_next!(iter.next() => "dir2/dir2.txt":FileMode::REG);
            check_next!(iter.next() => "dir2/nested":FileMode::DIR);
            check_next!(iter.next() => "dir2/nested/coolfile.txt":FileMode::REG);
            check_next!(iter.next() => "exec":FileMode::EXEC);
            check_next!(iter.next() => "test.txt":FileMode::REG);
            check_next!(iter.next() => "zs":FileMode::DIR);
            check_next!(iter.next() => "zs/one.txt":FileMode::REG);
            assert_eq!(iter.peek()?, None);
            assert_eq!(iter.next()?, None);
            Ok(())
        })
    })
}

#[test]
fn test_tree_iterator_collect_over_non_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            // step over root, "dir", and "dir/test.txt"
            iter.nth(2)?;
            let mut vec = vec![];
            iter.collect_over_tree(&mut vec)?;
            let paths = vec.iter().map(BitEntry::path).collect::<Vec<_>>();
            assert_eq!(paths, vec!["dir2/dir2.txt", "dir2/nested/coolfile.txt",]);
            Ok(())
        })
    })
}

#[test]
fn test_tree_tree_iterator_step_over_tree_twice() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        // this tree should match the directory structure below
        let oid = tree_oid! {
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
        };
        // this only tests `next` and not `peek` or `over`
        // we only compare paths as comparing modes is a bit pointless, and the the index may correctly have unknown oids
        let mut iter = repo.tree_iter(oid);
        check_next!(iter.next() => "":FileMode::DIR);
        check_next!(iter.over() => "dir0":FileMode::DIR);
        check_next!(iter.over() => "dir1":FileMode::DIR);
        check_next!(iter.over() => "dir2":FileMode::DIR);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_collect_over_root() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            let mut vec = vec![];
            iter.collect_over_tree(&mut vec)?;
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
    })
}

#[test]
fn test_tree_tree_iterator_matches_index_tree_iterator() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            // this tree should match the directory structure below
            let oid = tree_oid! {
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
            let index_tree_iter = index.tree_iter().map(|entry| Ok(entry.path()));
            assert!(tree_tree_iter.eq(index_tree_iter)?);
            Ok(())
        })
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
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
            check_next!(iter.next() => "": FileMode::DIR);
            check_next!(iter.next() => "dir": FileMode::DIR);
            check_next!(iter.next() => "dir/test.txt": FileMode::REG);
            check_next!(iter.next() => "dir2": FileMode::DIR);
            check_next!(iter.next() => "dir2/dir2.txt": FileMode::REG);
            check_next!(iter.next() => "dir2/nested": FileMode::DIR);
            check_next!(iter.next() => "dir2/nested/coolfile.txt": FileMode::REG);
            check_next!(iter.next() => "exec": FileMode::EXEC);
            check_next!(iter.next() => "test.txt": FileMode::REG);
            check_next!(iter.next() => "zs": FileMode::DIR);
            check_next!(iter.next() => "zs/one.txt": FileMode::REG);
            Ok(())
        })
    })
}

#[test]
fn test_index_tree_iterator_filter() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter().filter(|entry| Ok(entry.path().starts_with("dir2")));
            check_next!(iter.peek() => "dir2": FileMode::DIR);
            check_next!(iter.next() => "dir2": FileMode::DIR);
            check_next!(iter.next() => "dir2/dir2.txt": FileMode::REG);
            check_next!(iter.peek() => "dir2/nested": FileMode::DIR);
            check_next!(iter.over() => "dir2/nested": FileMode::DIR);
            assert!(iter.next()?.is_none());
            Ok(())
        })
    })
}
