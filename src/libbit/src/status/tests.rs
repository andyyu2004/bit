use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_status_untracked_files() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        touch!(repo: "bar");
        touch!(repo: "baz");
        bit_add!(repo: "bar");

        let untracked = repo.untracked_files()?;
        assert_eq!(untracked.len(), 2);
        assert_eq!(untracked[0], "baz");
        assert_eq!(untracked[1], "foo");
        Ok(())
    })
}

#[test]
fn test_status_modified_files() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        touch!(repo: "foo/baz");
        touch!(repo: "foo.l");
        bit_add_all!(repo);
        modify!(repo: "foo.l");
        modify!(repo: "foo/bar");

        let diff = repo.worktree_index_diff()?;
        assert_eq!(diff.modified.len(), 2);
        let mut modified = diff.modified.into_iter();
        assert_eq!(modified.next().unwrap(), "foo.l");
        assert_eq!(modified.next().unwrap(), "foo/bar");
        Ok(())
    })
}

#[test]
fn test_status_modified_then_reverted() -> BitResult<()> {
    // potential race conditions in here so we run it a few times to be surer
    for _ in 0..100 {
        BitRepo::with_test_repo(|repo| {
            mkdir!(repo: "foo");
            touch!(repo: "foo/bar");
            touch!(repo: "foo/baz");
            touch!(repo: "foo.l");
            modify!(repo: "foo/bar" < "original content");
            bit_add_all!(repo);
            modify!(repo: "foo.l");
            modify!(repo: "foo/bar" < "changed content");
            // revert foo/bar back to original contents
            modify!(repo: "foo/bar" < "original content");

            let diff = repo.worktree_index_diff()?;
            assert_eq!(diff.modified.len(), 1);
            let mut modified = diff.modified.into_iter();
            assert_eq!(modified.next().unwrap(), "foo.l");
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn test_status_modified_then_reverted_with_same_filesizes() -> BitResult<()> {
    for _ in 0..100 {
        BitRepo::with_test_repo(|repo| {
            mkdir!(repo: "foo");
            touch!(repo: "foo/bar");
            touch!(repo: "foo/baz");
            touch!(repo: "foo.l");
            modify!(repo: "foo/bar" < "abc");
            stat!(repo: "foo/bar");
            bit_add_all!(repo);
            modify!(repo: "foo.l");
            modify!(repo: "foo/bar" < "123");
            // revert foo/bar back to original contents
            modify!(repo: "foo/bar" < "abc");

            stat!(repo: "foo/bar");
            let diff = repo.worktree_index_diff()?;
            assert_eq!(diff.modified.len(), 1);
            let mut modified = diff.modified.into_iter();
            assert_eq!(modified.next().unwrap(), "foo.l");
            Ok(())
        })?;
    }
    Ok(())
}
