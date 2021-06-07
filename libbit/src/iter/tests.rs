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
        touch!(repo: "foo");
        symlink!(repo: "foo" <- "link");
        let entries = repo.worktree_iter()?.collect::<Vec<_>>()?;
        assert_eq!(entries.len(), 2);
        Ok(())
    })
}
