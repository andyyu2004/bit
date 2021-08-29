use super::*;
use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::signature::BitSignature;
use std::io::BufReader;
use std::str::FromStr;

#[test]
fn test_create_branch_on_empty_repo() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        repo.bit_create_branch("some-branch", &rev!("HEAD"))?;
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/some-branch"));
        Ok(())
    })
}

#[test]
fn test_calculate_ref_decoration() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        repo.bit_create_branch("a-new-branch", &rev!("HEAD"))?;
        repo.bit_create_branch("b-new-branch", &rev!("HEAD"))?;
        repo.bit_create_branch("c-new-branch", &rev!("HEAD"))?;
        let refs = repo.ls_refs()?;
        let decorations = repo.ref_decorations(&refs)?;
        assert_eq!(decorations.len(), 1);

        let mut values = decorations.values();
        let expected = btreeset! {
            RefDecoration::Symbolic(symbolic!("HEAD"), symbolic!("refs/heads/master")),
            RefDecoration::Branch(symbolic!("refs/heads/a-new-branch")),
            RefDecoration::Branch(symbolic!("refs/heads/b-new-branch")),
            RefDecoration::Branch(symbolic!("refs/heads/c-new-branch")),
        };
        let actual = values.next().unwrap();
        assert_eq!(actual, &expected);
        Ok(())
    })
}

#[test]
fn test_ls_refs_on_empty_repo() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let refs = repo.ls_refs()?;
        // although refs/heads/master is pointed to by HEAD it doesn't actually on the file system
        assert_eq!(refs, btreeset! { symbolic!("HEAD") });
        Ok(())
    })
}

#[test]
fn test_ls_refs_on_sample_repo() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let refs = repo.ls_refs()?;
        assert_eq!(
            refs,
            btreeset! {
                symbolic!("HEAD") ,
                symbolic!("refs/heads/master") ,
            }
        );
        Ok(())
    })
}

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        // repo initializes with `HEAD` pointing to `refs/heads/master`
        // resolving nonexistent symbolic ref should just return itself (minus the prefix)
        assert_eq!(repo.try_fully_resolve_ref(symbolic_ref!("ref: refs/heads/master"))?, None);
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
fn test_create_branch() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        repo.bit_create_branch("new-branch", &rev!("HEAD"))?;
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
fn test_reflog_contents_on_commit() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        // on initial commit
        touch!(repo: "foo");
        bit_commit_all!(repo);
        let expected_committer = repo.user_signature()?;
        let expected_message = "commit (initial): arbitrary message".to_owned();

        let head_reflog = repo.refdb()?.read_reflog(symbolic!("HEAD"))?;
        let master_reflog = repo.refdb()?.read_reflog(symbolic!("master"))?;

        assert_eq!(head_reflog.len(), 1);
        assert_eq!(head_reflog[0].committer, expected_committer);
        assert_eq!(head_reflog[0].message, expected_message);
        assert_eq!(master_reflog.len(), 1);
        assert_eq!(master_reflog[0].committer, expected_committer);
        assert_eq!(master_reflog[0].message, expected_message);

        // have to drop these otherwise the lockfile will prevent the next commit
        // (drops look nicer than scoping them with braces imo)
        drop(head_reflog);
        drop(master_reflog);

        touch!(repo: "bar");
        bit_commit_all!(repo);

        // on another commit
        // the key difference is that there is no initial note
        // and another detail is that HEAD's reflog is written to even though it's not actually moving itself
        let head_reflog = repo.refdb()?.read_reflog(symbolic!("HEAD"))?;
        let master_reflog = repo.refdb()?.read_reflog(symbolic!("master"))?;

        let expected_message = "commit: arbitrary message".to_owned();
        assert_eq!(head_reflog.len(), 2);
        assert_eq!(head_reflog[0].committer, expected_committer);
        assert_eq!(head_reflog[0].message, expected_message);
        assert_eq!(master_reflog.len(), 2);
        assert_eq!(master_reflog[0].committer, expected_committer);
        assert_eq!(master_reflog[0].message, expected_message);

        Ok(())
    })
}
