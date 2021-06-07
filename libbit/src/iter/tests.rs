use crate::error::BitResult;
use crate::iter::BitTreeIterator;
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
#[test]
fn test_index_tree_iterator_next() -> BitResult<()> {
    BitRepo::find(repos_dir!("indextest"), |repo| {
        repo.with_index(|index| {
            let mut iter = index.tree_iter();
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
