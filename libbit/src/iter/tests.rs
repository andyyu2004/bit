use crate::error::BitResult;
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
