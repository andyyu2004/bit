use crate::error::BitError;
use crate::repo::BitRepo;

#[test]
fn test_non_initial_empty_commit() {
    let err = BitRepo::with_sample_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.to_string(), BitError::EMPTY_COMMIT_CLEAN_WORKTREE);
}

#[test]
fn test_initial_empty_commit() {
    let err = BitRepo::with_empty_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.to_string(), BitError::EMPTY_COMMIT_EMPTY_WORKTREE);
}
