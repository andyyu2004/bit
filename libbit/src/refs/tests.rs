use super::BitReflogEntry;
use crate::error::BitResult;
use crate::refs::{is_valid_name, BitRef, BitRefDbBackend, BitReflog};
use crate::repo::{BitRepo, Repo};
use crate::serialize::{Deserialize, Serialize};
use crate::signature::BitSignature;
use crate::{error::BitErrorExt, obj::CommitMessage};
use std::io::BufReader;
use std::str::FromStr;

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
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
    BitRepo::with_empty_repo(|repo| {
        // it should only resolve until `refs/heads/master` as the branch file doesn't exist yet
        assert_eq!(repo.resolve_ref(BitRef::HEAD)?, symbolic_ref!("refs/heads/master"));
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        // HEAD -> `refs/heads/master` should exist on a non empty repo, then it should resolve to the oid contained within master
        assert_eq!(
            repo.resolve_ref(BitRef::HEAD)?,
            BitRef::Direct("902e59e7eadc1c44586354c9ecb3098fb316c2c4".into())
        );
        Ok(())
    })
}

#[test]
fn test_create_branch_in_fresh() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
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

#[test]
fn test_parse_reflog() {
    let s = include_str!("../../tests/files/sample-reflog");
    BitReflog::from_str(s).expect("failed to parse valid reflog");
}

#[test]
fn test_parse_reflog_entry() {
    let s = "95a612b0afcae388c4f9fb9ddf4dba489919b766 4f0b23654b5ffc3a994ec4bf0212ed8dc4358400 Andy Yu <andyyu2004@gmail.com> 1622453485 +1200	commit: some commit message";
    let entry = BitReflogEntry::from_str(s).unwrap();
    assert_eq!(
        entry,
        BitReflogEntry {
            old_oid: "95a612b0afcae388c4f9fb9ddf4dba489919b766".into(),
            new_oid: "4f0b23654b5ffc3a994ec4bf0212ed8dc4358400".into(),
            committer: BitSignature::from_str("Andy Yu <andyyu2004@gmail.com> 1622453485 +1200")
                .unwrap(),
            message: "commit: some commit message".into(),
        }
    );
}

#[test]
fn test_deserialize_then_reserialize_reflog() -> BitResult<()> {
    let bytes = &include_bytes!("../../tests/files/sample-reflog")[..];
    let mut reader = BufReader::new(bytes);
    let reflog = BitReflog::deserialize(&mut reader)?;
    let mut buf = vec![];
    reflog.serialize(&mut buf)?;

    assert_eq!(bytes, &buf);
    Ok(())
}

#[test]
fn test_reflog_contents_on_initial_commit() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "foo");
        bit_commit_all!(repo);
        let head_reflog = repo.refdb().read_reflog(symbolic!("HEAD"))?;
        let master_reflog = repo.refdb().read_reflog(symbolic!("refs/heads/master"))?;

        let expected_committer = repo.user_signature()?;
        let expected_message = "commit (initial): arbitrary message".to_owned();
        assert_eq!(head_reflog.len(), 1);
        assert_eq!(head_reflog[0].committer, expected_committer);
        assert_eq!(head_reflog[0].message, expected_message);
        assert_eq!(master_reflog.len(), 1);
        assert_eq!(master_reflog[0].committer, expected_committer);
        assert_eq!(master_reflog[0].message, expected_message);
        Ok(())
    })
}
