use super::*;

macro_rules! lex {
    ($rev:expr) => {
        RevspecLexer::new($rev).collect::<Vec<_>>().expect("failed to lex revspec")
    };
}

macro_rules! parse_rev {
    ($rev:expr) => {
        Revspec::from_str($rev).expect("failed to parse revspec")
    };
}

#[test]
fn test_lex_simple_revspec() {
    let tokens = lex!("HEAD^");
    assert_eq!(tokens, vec![Token::Ref(symbolic_ref!("HEAD")), Token::Caret]);
}

#[test]
fn test_parse_revspec_parent() {
    let rev = parse_rev!("HEAD^");
    assert_eq!(rev, Revspec::Parent(Box::new(Revspec::Ref(symbolic_ref!("HEAD")))))
}

#[test]
fn test_lex_revspec_with_symref_ancestor() {
    let tokens = lex!("HEAD~5");
    assert_eq!(tokens, vec![Token::Ref(symbolic_ref!("HEAD")), Token::Tilde, Token::Num(5)]);
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
    Ok(())
    // BitRepo::init_load("path", |repo| Ok(()))
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
