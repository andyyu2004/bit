use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_status_untracked_files() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        touch!(repo: "bar");
        touch!(repo: "baz");
        bit_add!(repo: "bar");

        let diff = repo.diff_index_worktree()?;
        assert!(diff.modified.is_empty());
        assert_eq!(diff.untracked.len(), 2);
        assert_eq!(diff.untracked[0], "baz");
        assert_eq!(diff.untracked[1], "foo");
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

        let diff = repo.diff_index_worktree()?;
        assert!(diff.untracked.is_empty());
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

            let diff = repo.diff_index_worktree()?;
            assert!(diff.untracked.is_empty());
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
            bit_add_all!(repo);
            modify!(repo: "foo.l");
            modify!(repo: "foo/bar" < "123");
            // revert foo/bar back to original contents
            modify!(repo: "foo/bar" < "abc");

            let diff = repo.diff_index_worktree()?;
            assert_eq!(diff.modified.len(), 1);
            let mut modified = diff.modified.into_iter();
            assert_eq!(modified.next().unwrap(), "foo.l");
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn test_status_on_symlink() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        modify!(repo: "foo" < "some content that is not the same size as the symlink itself");
        symlink!(repo: "foo" <- "link");
        bit_add_all!(repo);
        bit_commit!(repo);
        let diff = repo.diff_index_worktree()?;
        assert_eq!(diff.modified.len(), 0);
        assert_eq!(diff.untracked.len(), 0);
        Ok(())
    })
}

#[test]
fn test_status_staged_modified_files() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        modify!(repo: "foo");
        bit_add!(repo: "foo");
        let diff = repo.diff_head_index()?;
        // assert!(diff.deleted.is_empty());
        assert!(diff.new.is_empty());
        assert_eq!(diff.staged.len(), 1);
        assert_eq!(diff.staged[0], "foo");
        Ok(())
    })
}

#[test]
fn test_status_staged_new_files_simple() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "new");
        bit_add!(repo: "new");
        let diff = repo.diff_head_index()?;
        assert!(diff.deleted.is_empty());
        assert!(diff.staged.is_empty());
        assert_eq!(diff.new.len(), 1);
        assert_eq!(diff.new[0], "new");
        Ok(())
    })
}

// #[test]
// fn test_status_staged_deleted_files() -> BitResult<()> {
//     BitRepo::with_sample_repo(|repo| {
//         rm!(repo: "foo");
//         bit_add!(repo: "foo");
//         let diff = repo.diff_head_index()?;
//         assert!(diff.new.is_empty());
//         assert!(diff.staged.is_empty());
//         assert_eq!(diff.deleted.len(), 1);
//         assert_eq!(diff.deleted[0], "foo");
//         Ok(())
//     })
// }

// #[test]
// fn test_status_staged_deleted_directory() -> BitResult<()> {
//     BitRepo::with_sample_repo(|repo| {
//         rm!(repo: "new");
//         bit_add!(repo: "new");
//         let diff = repo.diff_head_index()?;
//         // assert!(diff.deleted.is_empty());
//         assert!(diff.staged.is_empty());
//         assert_eq!(diff.new.len(), 1);
//         assert_eq!(diff.new[0], "new");
//         Ok(())
//     })
// }

#[test]
fn test_status_staged_new_files_no_head() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        touch!(repo: "bar");
        bit_add!(repo: "foo");
        let diff = repo.diff_head_index()?;
        assert!(diff.deleted.is_empty());
        assert!(diff.staged.is_empty());
        assert_eq!(diff.new.len(), 1);
        assert_eq!(diff.new[0], "foo");
        Ok(())
    })
}
