use fallible_iterator::FallibleIterator;

use crate::error::BitResult;
use crate::obj::FileMode;
use crate::repo::BitRepo;

impl BitRepo {
    /// be careful when deleting `rm foo` as the symlink points at it
    pub fn with_sample_repo<R>(f: impl FnOnce(&Self) -> BitResult<R>) -> BitResult<R> {
        Self::with_test_repo(|repo| {
            touch!(repo: "foo");
            touch!(repo: "bar");
            mkdir!(repo: "dir");
            mkdir!(repo: "dir/bar");
            touch!(repo: "dir/baz");
            touch!(repo: "dir/bar.l");
            touch!(repo: "dir/bar/qux");
            symlink!(repo: "bar" <- "dir/link");

            bit_add_all!(repo);
            bit_commit!(repo);
            f(repo)
        })
    }
}

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
