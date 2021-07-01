use crate::error::{BitErrorExt, BitResult};
use crate::repo::BitRepo;

#[test]
fn test_non_initial_empty_commit() -> BitResult<()> {
    let status = BitRepo::with_sample_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err()
    .into_status_error()?;

    assert!(status.is_empty());
    assert!(!status.is_initial());
    Ok(())
}

#[test]
fn test_initial_empty_commit() -> BitResult<()> {
    let status = BitRepo::with_empty_repo(|repo| {
        bit_commit_all!(repo);
        Ok(())
    })
    .unwrap_err()
    .into_status_error()?;

    assert!(status.is_empty());
    assert!(status.is_initial());
    Ok(())
}
