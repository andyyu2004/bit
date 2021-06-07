use crate::error::BitResult;
use crate::iter::TreeIterator;
use crate::obj::{FileMode, TreeEntry};
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

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
fn test_tree_iterator_step_over() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
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
        check_next!(iter.peek() => "bar":FileMode::REG);
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::DIR);
        check_next!(iter.next() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_tree_iterator_peekable_step_over_peeked() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let mut iter = repo.head_tree_iter()?;
        check_next!(iter.peek() => "bar":FileMode::REG);
        // remember peeked value
        check_next!(iter.over() => "bar": FileMode::REG);
        check_next!(iter.over() => "dir":FileMode::DIR);
        check_next!(iter.over() => "foo": FileMode::REG);
        assert_eq!(iter.next()?, None);
        Ok(())
    })
}

#[test]
fn test_worktree_iterator_reads_symlinks() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "foo");
        symlink!(repo: "foo" <- "link");
        let entries = repo.worktree_iter()?.collect::<Vec<_>>()?;
        assert_eq!(entries.len(), 2);
        Ok(())
    })
}

#[test]
fn test_index_tree_iterator_step_over() -> BitResult<()> {
    // TODO can't actually run these concurrently on the same repo...
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
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

// careful of the following scenario that foo is only yielded once as a pseudotree
// we keep track of all yielded pseudotrees not just the previous one to avoid any issues
/// ├── a
/// │  └── foo
/// │  └── b
/// │      └── bar
/// │  └── qux
#[test]
fn test_index_tree_iterator_next() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let iter = index.tree_iter();
            let entries = iter.collect::<Vec<_>>()?;
            let expected = vec![
                TreeEntry {
                    mode: FileMode::DIR,
                    path: "dir".into(),
                    oid: "0000000000000000000000000000000000000000".into(),
                },
                TreeEntry {
                    mode: FileMode::REG,
                    path: "dir/test.txt".into(),
                    oid: "ce013625030ba8dba906f756967f9e9ca394464a".into(),
                },
                TreeEntry {
                    mode: FileMode::DIR,
                    path: "dir2".into(),
                    oid: "0000000000000000000000000000000000000000".into(),
                },
                TreeEntry {
                    mode: FileMode::REG,
                    path: "dir2/dir2.txt".into(),
                    oid: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".into(),
                },
                TreeEntry {
                    mode: FileMode::DIR,
                    path: "dir2/nested".into(),
                    oid: "0000000000000000000000000000000000000000".into(),
                },
                TreeEntry {
                    mode: FileMode::REG,
                    path: "dir2/nested/coolfile.txt".into(),
                    oid: "d9d0fd07b0c6e36d2db0db2e9a3f4918622c65dc".into(),
                },
                TreeEntry {
                    mode: FileMode::EXEC,
                    path: "exec".into(),
                    oid: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".into(),
                },
                TreeEntry {
                    mode: FileMode::REG,
                    path: "test.txt".into(),
                    oid: "ce013625030ba8dba906f756967f9e9ca394464a".into(),
                },
                TreeEntry {
                    mode: FileMode::DIR,
                    path: "zs".into(),
                    oid: "0000000000000000000000000000000000000000".into(),
                },
                TreeEntry {
                    mode: FileMode::REG,
                    path: "zs/one.txt".into(),
                    oid: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".into(),
                },
            ];
            assert_eq!(entries, expected);
            Ok(())
        })
    })
}
