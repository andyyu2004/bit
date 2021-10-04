use crate::error::BitResult;
use crate::refs::BitRef;
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

    assert_eq!(std::fs::read_dir(tmpdir.path())?.count(), 23);

    BitRepo::find(tmpdir.path(), |repo| {
        let fetch_summary = repo.fetch_blocking(DEFAULT_REMOTE)?;
        assert!(matches!(fetch_summary.status, FetchStatus::UpToDate));
        assert_eq!(repo.read_head()?, BitRef::MASTER);
        Ok(())
    })
}

#[test]
fn test_clone_empty_repo() -> BitResult<()> {
    let remote = tempfile::tempdir()?;
    let local = tempfile::tempdir()?;
    BitRepo::init(remote.path())?;
    BitRepo::clone_blocking(local.path(), remote.path().to_str().unwrap())
}

#[test]
fn test_clone_dont_cleanup_existing_directory_on_failure() -> BitResult<()> {
    let tmpdir = tempfile::tempdir()?;
    BitRepo::clone_blocking(tmpdir.path(), "/dev/null").unwrap_err();
    assert!(tmpdir.path().exists());
    Ok(())
}

#[test]
fn test_clone_cleanup_created_directory_on_failure() -> BitResult<()> {
    let tmpdir = tempfile::tempdir()?;
    let repo_path = tmpdir.path().join("myrepo");
    BitRepo::clone_blocking(&repo_path, "/dev/null").unwrap_err();
    assert!(!repo_path.exists());
    Ok(())
}
