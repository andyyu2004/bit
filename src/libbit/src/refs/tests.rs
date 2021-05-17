use super::is_valid_name;
use crate::error::{BitErrorExt, BitResult};
use crate::refs::{BitRef, SymbolicRef};
use crate::repo::BitRepo;

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        // repo initializes with `HEAD` pointing to `refs/heads/master`
        // resolving nonexistent symbolic ref should just return itself (minus the prefix)
        assert_eq!(
            repo.resolve_ref(symbolic_ref!("ref: refs/heads/master"))?,
            symbolic_ref!("refs/heads/master"),
        );
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref_in_fresh_repo() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        // it should only resolve until `refs/heads/master` as the branch file doesn't exist yet
        assert_eq!(repo.resolve_ref(HEAD!())?, symbolic_ref!("refs/heads/master"));
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        // HEAD -> `refs/heads/master` should exist on a non empty repo, then it should resolve to the oid contained within master
        assert_eq!(
            repo.resolve_ref(HEAD!())?,
            BitRef::Direct("902e59e7eadc1c44586354c9ecb3098fb316c2c4".into())
        );
        Ok(())
    })
}

#[test]
fn test_create_branch_in_fresh() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        let err = repo.bit_create_branch("new-branch").unwrap_err();
        assert_eq!(err.into_nonexistent_symref_err()?, symbolic!("refs/heads/master"));
        Ok(())
    })
}

#[test]
fn test_create_branch() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        repo.bit_create_branch("new-branch")?;
        Ok(())
    })
}

#[test]
fn test_branch_regex() {
    assert!(is_valid_name("sometext"));
    assert!(!is_valid_name(".test"));
    assert!(!is_valid_name("test.."));
    assert!(!is_valid_name("tes t"));
    assert!(!is_valid_name("tes~y"));
    assert!(!is_valid_name("te*s"));
    assert!(!is_valid_name("file.lock"));
    assert!(!is_valid_name("file@{}"));
    assert!(!is_valid_name("caret^"));
    assert!(!is_valid_name("badendingslash/"));
    assert!(!is_valid_name("bads/.dot"));
}
