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
        assert_eq!(diff.new.len(), 2);
        assert_eq!(diff.new[0].filepath, "baz");
        assert_eq!(diff.new[1].filepath, "foo");
        Ok(())
    })
}

#[test]
fn test_status_add_and_delete_file() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        bit_add_all!(repo);
        rm!(repo: "foo");

        let diff = repo.status_report()?;
        assert!(diff.staged.modified.is_empty());
        assert!(diff.staged.deleted.is_empty());
        assert!(diff.unstaged.new.is_empty());
        assert!(diff.unstaged.modified.is_empty());

        assert_eq!(diff.staged.new.len(), 1);
        assert_eq!(diff.unstaged.deleted.len(), 1);
        assert_eq!(diff.staged.new[0].filepath, "foo");
        assert_eq!(diff.unstaged.deleted[0].filepath, "foo");
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
        assert!(diff.new.is_empty());
        assert_eq!(diff.modified.len(), 2);
        let mut modified = diff.modified.into_iter();
        assert_eq!(modified.next().unwrap().1.filepath, "foo.l");
        assert_eq!(modified.next().unwrap().1.filepath, "foo/bar");
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
            assert!(diff.new.is_empty());
            assert_eq!(diff.modified.len(), 1);
            let mut modified = diff.modified.into_iter();
            assert_eq!(modified.next().unwrap().1.filepath, "foo.l");
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
            assert_eq!(modified.next().unwrap().1.filepath, "foo.l");
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
        assert_eq!(diff.new.len(), 0);
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
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.modified[0].1.filepath, "foo");
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
        assert!(diff.modified.is_empty());
        assert_eq!(diff.new.len(), 1);
        assert_eq!(diff.new[0].filepath, "new");
        Ok(())
    })
}

#[test]
fn test_mode_change_is_detected() {
}

#[test]
fn test_status_staged_deleted_directory() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        rmdir!(repo: "dir");
        bit_add_all!(repo);
        let diff = repo.diff_head_index()?;
        assert!(diff.modified.is_empty());
        assert!(diff.new.is_empty());
        assert_eq!(diff.deleted.len(), 4);

        let mut iter = diff.deleted.into_iter();
        assert_eq!(iter.next().unwrap().filepath, "dir/bar.l");
        assert_eq!(iter.next().unwrap().filepath, "dir/bar/qux");
        assert_eq!(iter.next().unwrap().filepath, "dir/baz");
        assert_eq!(iter.next().unwrap().filepath, "dir/link");
        Ok(())
    })
}

#[test]
fn test_status_staged_new_files_no_head() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        touch!(repo: "bar");
        bit_add!(repo: "foo");
        let diff = repo.diff_head_index()?;
        assert!(diff.deleted.is_empty());
        assert!(diff.modified.is_empty());
        assert_eq!(diff.new.len(), 1);
        assert_eq!(diff.new[0].filepath, "foo");
        Ok(())
    })
}
