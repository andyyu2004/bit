use super::*;

#[test]
fn test_parse_revspec_parent() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD^");
        assert_eq!(
            rev.parse(repo)?,
            &Revspec::Parent(Box::new(Revspec::Ref(symbolic_ref!("HEAD"))))
        );
        Ok(())
    })
}

#[test]
fn test_parse_at_symbol_as_alias_to_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        assert_eq!(rev!("@").parse(repo)?, rev!("HEAD").parse(repo)?);
        assert_eq!(rev!("@^").parse(repo)?, rev!("HEAD^").parse(repo)?);
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_symref_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD~5");
        assert_eq!(
            rev.parse(repo)?,
            &Revspec::Ancestor(Box::new(Revspec::Ref(symbolic_ref!("HEAD"))), 5)
        );
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_symref() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let rev = rev!("e3eaee01f47f98216f4160658179420ff5e30f50");
        assert_eq!(
            rev.parse(repo)?,
            &Revspec::Ref(BitRef::Direct("e3eaee01f47f98216f4160658179420ff5e30f50".into()))
        );
        Ok(())
    })
}

#[test]
fn test_resolve_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 1]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 2]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_expansion_master() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let master_oid = repo.resolve_rev(&rev!("master"))?;
        let head_oid = repo.resolve_rev(&rev!("HEAD"))?;
        assert_eq!(master_oid, head_oid);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_double_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD^^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 3]);
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD~4");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_complex_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD~2^^");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_parent_of_non_commit_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        let rev = rev!("ebc3780a093cbda629d531c1c0d530a82063ee6f^");
        let err = repo.resolve_rev(&rev).unwrap_err();
        assert_eq!(
            err.to_string(),
            "object `ebc3780a093cbda629d531c1c0d530a82063ee6f` is a tree, not a commit".to_string()
        );
        Ok(())
    })
}

#[test]
fn test_resolve_non_commit_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        let rev = rev!("ebc3780a093cbda629d531c1c0d530a82063ee6f");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, "ebc3780a093cbda629d531c1c0d530a82063ee6f".into());
        Ok(())
    })
}

#[test]
fn test_resolve_partial_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        let rev = rev!("ebc3780");
        let oid = repo.resolve_rev(&rev)?;
        assert_eq!(oid, "ebc3780a093cbda629d531c1c0d530a82063ee6f".into());
        Ok(())
    })
}

#[test]
fn test_resolve_revspec_non_existent_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD~2000");
        let err = repo.resolve_rev(&rev).unwrap_err();
        assert_eq!(
            err.to_string(),
            "revision `HEAD~2000` refers to the parent of an initial commit"
        );
        Ok(())
    })
}
