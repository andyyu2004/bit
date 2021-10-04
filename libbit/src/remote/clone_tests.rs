use crate::error::BitResult;
use crate::remote::{FetchStatus, DEFAULT_REMOTE};
use crate::repo::BitRepo;

#[test]
fn test_fetch() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let remote_path = repos_dir!("logic");
        repo.add_remote("origin", &remote_path)?;
        repo.fetch_blocking("origin")?;
        Ok(())
    })
}

#[test]
fn test_clone() -> BitResult<()> {
    let remote_path = repos_dir!("ribble");
    let tmpdir = tempfile::tempdir()?;
    BitRepo::clone_blocking(tmpdir.path(), &remote_path)?;
    BitRepo::find(tmpdir.path(), |repo| {
        let fetch_summary = repo.fetch_blocking(DEFAULT_REMOTE)?;
        assert!(matches!(fetch_summary.status, FetchStatus::UpToDate));
        Ok(())
    })
}
