use crate::error::{BitErrorExt, BitResult};
use crate::obj::BitObject;
use crate::repo::BitRepo;

#[test]
fn test_commit_in_detached_head_state() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let revision = rev!("HEAD^");
        let oid = repo.fully_resolve_rev(&revision)?;
        repo.checkout(&revision)?;

        touch!(repo: "newfile");
        modify!(repo: "bar");
        let summary = bit_commit_all!(repo);

        assert_eq!(oid, summary.commit.parent.unwrap());
        assert_eq!(summary.commit.oid(), repo.read_head()?.into_direct());
        Ok(())
    })
}

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
