use super::*;

macro_rules! parse_rev {
    ($rev:expr) => {
        Revspec::from_str($rev).expect("failed to parse revspec")
    };
}

#[test]
fn test_parse_revspec_parent() {
    let rev = parse_rev!("HEAD^");
    assert_eq!(rev, Revspec::Parent(Box::new(Revspec::Ref(symbolic_ref!("HEAD")))))
}

#[test]
fn test_parse_revspec_with_symref_ancestor() {
    let rev = parse_rev!("HEAD~5");
    assert_eq!(rev, Revspec::Ancestor(Box::new(Revspec::Ref(symbolic_ref!("HEAD"))), 5));
}

#[test]
fn test_parse_revspec_with_symref() {
    let rev = parse_rev!("e3eaee01f47f98216f4160658179420ff5e30f50");
    assert_eq!(rev, Revspec::Ref(BitRef::Direct("e3eaee01f47f98216f4160658179420ff5e30f50".into())))
}

#[test]
fn test_resolve_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 1]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 2]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_double_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD^^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 3]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD~4");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_complex_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD~2^^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_non_commit_ref() -> BitResult<()> {
    BitRepo::find("tests/repos/ribble", |repo| {
        let rev = parse_rev!("9fd6fa21a285cf44e5a3f0469992e4ec6bb9a845");
        let err = repo.resolve_rev(&rev).unwrap_err();
        assert_eq!(err.to_string(), format!("object `{}` is a tree, not a commit", rev),);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_non_existent_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = parse_rev!("HEAD~2000");
        let err = repo.resolve_rev(&rev).unwrap_err();
        assert_eq!(
            err.to_string(),
            "revision `HEAD~2000` refers to the parent of an initial commit"
        );
        Ok(())
    })
}
