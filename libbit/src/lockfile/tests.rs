use super::Filelock;
use crate::error::BitResult;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_filelock_removes_lockfile_on_drop_on_commit() -> BitResult<()> {
    let dir = tempdir()?;
    let path = dir.path().join("foo");
    let mut filelock = Filelock::<Vec<u8>>::lock(&path)?;
    write!(filelock, "random stuff")?;
    drop(filelock);

    assert_eq!(std::fs::read_to_string(&path)?, "random stuff");
    assert!(!path.with_extension("lock").exists());
    Ok(())
}

#[test]
fn test_filelock_removes_lockfile_on_drop_on_rollback() -> BitResult<()> {
    let dir = tempdir()?;
    let path = dir.path().join("foo");
    let mut filelock = Filelock::<Vec<u8>>::lock(&path)?;
    write!(filelock, "random stuff 2")?;
    filelock.rollback();
    drop(filelock);

    // should be fine to create a file but leave it empty on rollback?
    assert!(!path.exists());
    assert!(!path.with_extension("lock").exists());
    Ok(())
}
