use super::*;

macro_rules! parse_rev {
    ($rev:expr) => {
        // NOTE: `eval` must be called with a repository in scope (tls)
        LazyRevspec::from_str($rev)?
    };
}

#[test]
fn test_parse_revspec_parent() -> BitResult<()> {
    BitRepo::with_test_repo(|_repo| {
        let rev = parse_rev!("HEAD^");
        assert_eq!(rev.eval()?, &Revspec::Parent(Box::new(Revspec::Ref(symbolic_ref!("HEAD")))));
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_symref_ancestor() -> BitResult<()> {
    BitRepo::with_test_repo(|_repo| {
        let rev = parse_rev!("HEAD~5");
        assert_eq!(
            rev.eval()?,
            &Revspec::Ancestor(Box::new(Revspec::Ref(symbolic_ref!("HEAD"))), 5)
        );
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_symref() -> BitResult<()> {
    BitRepo::with_test_repo(|_repo| {
        let rev = parse_rev!("e3eaee01f47f98216f4160658179420ff5e30f50");
        assert_eq!(
            rev.eval()?,
            &Revspec::Ref(BitRef::Direct("e3eaee01f47f98216f4160658179420ff5e30f50".into()))
        );
        Ok(())
    })
}

#[test]
fn test_resolve_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD");
        let oid = repo.resolve_rev(rev.eval()?)?;
        assert_eq!(oid, commits[commits.len() - 1]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD^");
        let oid = repo.resolve_rev(rev.eval()?)?;
        assert_eq!(oid, commits[commits.len() - 2]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_double_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD^^");
        let oid = repo.resolve_rev(rev.eval()?)?;
        assert_eq!(oid, commits[commits.len() - 3]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD~4");
        let oid = repo.resolve_rev(rev.eval()?)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_complex_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = parse_rev!("HEAD~2^^");
        let oid = repo.resolve_rev(rev.eval()?)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_non_commit_ref() -> BitResult<()> {
    BitRepo::find("tests/repos/ribble", |repo| {
        let rev = parse_rev!("ebc3780a093cbda629d531c1c0d530a82063ee6f");
        let err = repo.resolve_rev(rev.eval()?).unwrap_err();
        assert_eq!(err.to_string(), format!("object `{}` is a tree, not a commit", rev.eval()?));
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_non_existent_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = parse_rev!("HEAD~2000");
        let err = repo.resolve_rev(rev.eval()?).unwrap_err();
        assert_eq!(
            err.to_string(),
            "revision `HEAD~2000` refers to the parent of an initial commit"
        );
        Ok(())
    })
}
