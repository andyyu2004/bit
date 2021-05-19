use fallible_iterator::FallibleIterator;

use crate::error::BitResult;
use crate::obj::FileMode;
use crate::repo::BitRepo;

macro_rules! check_entry {
    ($next:expr => $path:literal:$mode:expr) => {
        let entry = $next?.unwrap();
        assert_eq!(entry.filepath, $path);
        assert_eq!(entry.mode, $mode);
    };
}

#[test]
fn test_head_iterator() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let entries = repo.head_iter()?.collect::<Vec<_>>()?;
        assert_eq!(entries.len(), 6);
        assert!(entries.is_sorted());

        let mut iter = repo.head_iter()?;
        check_entry!(iter.next() => "bar":FileMode::REG);
        check_entry!(iter.next() => "dir/bar.l":FileMode::REG);
        check_entry!(iter.next() => "dir/bar/qux":FileMode::REG);
        check_entry!(iter.next() => "dir/baz":FileMode::REG);
        check_entry!(iter.next() => "dir/link":FileMode::LINK);
        Ok(())
    })
}

#[test]
fn test_worktree_iterator_reads_symlinks() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        symlink!(repo: "foo" <- "link");
        let entries = repo.worktree_iter()?.collect::<Vec<_>>()?;
        assert_eq!(entries.len(), 2);
        Ok(())
    })
}
