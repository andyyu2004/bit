use super::BitEntry;
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::path::BitPath;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

impl PartialOrd for BitIndexEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitIndexEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        BitPath::path_cmp(self.sort_path().as_ref(), other.sort_path().as_ref())
            .then_with(|| self.stage().cmp(&other.stage()))
    }
}

#[test]
fn test_head_iterator() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let entries = repo.head_iter()?.collect::<Vec<_>>()?;
        assert_eq!(entries.len(), 6);
        assert!(entries.is_sorted());

        let mut iter = repo.head_iter()?;
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.next() => "dir/bar.l":FileMode::REG);
        check_next!(iter.next() => "dir/bar/qux":FileMode::REG);
        check_next!(iter.next() => "dir/baz":FileMode::REG);
        check_next!(iter.next() => "dir/link":FileMode::LINK);
        check_next!(iter.next() => "foo":FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_worktree_iterator_reads_symlinks() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        repo.with_index(|index| {
            touch!(repo: "foo");
            symlink!(repo: "foo" <- "link");
            let entries = index.worktree_iter()?.collect::<Vec<_>>()?;
            assert_eq!(entries.len(), 2);
            Ok(())
        })
    })
}

#[test]
fn test_simple_root_gitignore_file() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        repo.with_index(|index| {
            gitignore!(repo: {
                "ignore"
            });
            touch!(repo: "ignore");
            let entries = index.worktree_iter()?.collect::<Vec<_>>()?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].path, ".gitignore");
            Ok(())
        })
    })
}

#[test]
fn test_root_gitignore_ignore_self() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        repo.with_index(|index| {
            gitignore!(repo: {
                ".gitignore"
            });
            assert_eq!(index.worktree_iter()?.count()?, 0);
            Ok(())
        })
    })
}

#[test]
fn test_simple_root_gitignore_ignore_directory() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        repo.with_index(|index| {
            gitignore!(repo: {
                "ignore"
                ".gitignore"
            });
            mkdir!(repo: "ignore");
            touch!(repo: "ignore/a");
            touch!(repo: "ignore/b");
            touch!(repo: "ignore/c");
            assert_eq!(index.worktree_iter()?.count()?, 0);
            Ok(())
        })
    })
}

macro_rules! next_entries {
    ($iter:expr) => {{
        let entries = $iter.next()?.unwrap();
        entries.map(|entry| entry.map(|entry| entry.path.as_str()))
    }};
}

#[test]
fn test_walk_three_iterators() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let tree_a = tree! {
            a {
                b
            }
        };

        let tree_b = tree! {
            a {
                c
            }
            b {
                d
            }
        };

        let tree_c = tree! {
            b {
                c
            }
        };

        let mut iter = repo.walk_iterators([
            Box::new(repo.tree_iter(tree_a)),
            Box::new(repo.tree_iter(tree_b)),
            Box::new(repo.tree_iter(tree_c)),
        ]);

        assert_eq!(next_entries!(iter), [Some(""), Some(""), Some("")]);
        assert_eq!(next_entries!(iter), [Some("a"), Some("a"), None]);
        assert_eq!(next_entries!(iter), [Some("a/b"), None, None]);
        assert_eq!(next_entries!(iter), [None, Some("a/c"), None]);
        assert_eq!(next_entries!(iter), [None, Some("b"), Some("b")]);
        assert_eq!(next_entries!(iter), [None, None, Some("b/c")]);
        assert_eq!(next_entries!(iter), [None, Some("b/d"), None]);
        assert!(iter.next()?.is_none());

        Ok(())
    })
}

#[test]
fn test_walk_three_iterators_steps_over_same_tree() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let tree_a = tree! {
            a
            x {
                b {
                    c
                }
                d
            }
            z
        };

        let tree_b = tree! {
            b
            x {
                b {
                    c
                }
                d
            }
            z
        };

        let tree_c = tree! {
            a
            x {
                b {
                    c
                }
                d
            }
            y
            z
        };

        let mut iter = repo.walk_iterators([
            Box::new(repo.tree_iter(tree_a)),
            Box::new(repo.tree_iter(tree_b)),
            Box::new(repo.tree_iter(tree_c)),
        ]);

        assert_eq!(next_entries!(iter), [Some(""), Some(""), Some("")]);
        assert_eq!(next_entries!(iter), [Some("a"), None, Some("a")]);
        assert_eq!(next_entries!(iter), [None, Some("b"), None]);
        // should step over entire tree as they should have the same oid
        assert_eq!(next_entries!(iter), [Some("x"), Some("x"), Some("x")]);
        assert_eq!(next_entries!(iter), [None, None, Some("y")]);
        assert_eq!(next_entries!(iter), [Some("z"), Some("z"), Some("z")]);
        assert!(iter.next()?.is_none());

        Ok(())
    })
}
