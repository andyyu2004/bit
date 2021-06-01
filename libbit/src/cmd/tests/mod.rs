use crate::repo::BitRepo;

#[test]
fn test_non_initial_empty_commit() {
    let err = BitRepo::with_sample_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "nothing to commit");
}

#[test]
fn test_initial_empty_commit() {
    let err = BitRepo::with_empty_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "nothing to commit (create/copy files and use `bit add` to track)");
}