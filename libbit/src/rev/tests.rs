use crate::error::BitErrorExt;
use crate::obj::BitObjType;

use super::*;

#[test]
fn test_parse_revspec_reflog() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("@@{5}");
        assert_eq!(
            rev.parse(&repo)?,
            &ParsedRevspec::Reflog(Box::new(ParsedRevspec::Ref(symbolic_ref!("HEAD"))), 5)
        );
        Ok(())
    })
}

#[test]
fn test_parse_revspec_parent() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD^");
        assert_eq!(
            rev.parse(&repo)?,
            &ParsedRevspec::Parent(Box::new(ParsedRevspec::Ref(symbolic_ref!("HEAD"))), 1)
        );
        Ok(())
    })
}

#[test]
fn test_parse_at_symbol_as_alias_to_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        assert_eq!(rev!("@").parse(&repo)?, rev!("HEAD").parse(&repo)?);
        assert_eq!(rev!("@^").parse(&repo)?, rev!("HEAD^").parse(&repo)?);
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_symref_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD~5");
        assert_eq!(
            rev.parse(&repo)?,
            &ParsedRevspec::Ancestor(Box::new(ParsedRevspec::Ref(symbolic_ref!("HEAD"))), 5)
        );
        Ok(())
    })
}

#[test]
fn test_parse_revspec_with_oid() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let empty_oid = Oid::EMPTY_BLOB.to_string();
        let rev = rev!(&empty_oid);
        assert_eq!(rev.parse(&repo)?, &ParsedRevspec::Ref(BitRef::Direct(Oid::EMPTY_BLOB)));
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 1]);
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec_first_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD^");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 2]);
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec_expansion_master() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let master_oid = repo.fully_resolve_rev(&rev!("master"))?;
        let head_oid = repo.fully_resolve_rev(&rev!("HEAD"))?;
        assert_eq!(master_oid, head_oid);
        Ok(())
    })
}

#[test]
fn test_0th_parent_is_noop() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD^0");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, *commits.last().unwrap());
        Ok(())
    })
}

#[test]
fn test_0th_ancestor_is_noop() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD~0");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, *commits.last().unwrap());
        Ok(())
    })
}

#[test]
fn test_ancestor_defaults_to_first_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, _| {
        let rev0 = rev!("HEAD^");
        let rev1 = rev!("HEAD^1");
        assert_eq!(repo.fully_resolve_rev(&rev0)?, repo.fully_resolve_rev(&rev1)?);
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec_double_parent() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD^^");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 3]);
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD~4");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_complex_revspec() -> BitResult<()> {
    BitRepo::with_sample_repo_commits(|repo, commits| {
        let rev = rev!("HEAD~2^^");
        let oid = repo.fully_resolve_rev(&rev)?;
        assert_eq!(oid, commits[commits.len() - 5]);
        Ok(())
    })
}

#[test]
fn test_resolve_parent_of_non_commit_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        let rev = rev!("ebc3780a093cbda629d531c1c0d530a82063ee6f^");
        let (oid, obj_type) =
            repo.fully_resolve_rev_to_any(&rev).unwrap_err().try_into_expected_commit_error()?;
        assert_eq!(oid, "ebc3780a093cbda629d531c1c0d530a82063ee6f".into());
        assert_eq!(obj_type, BitObjType::Tree);
        Ok(())
    })
}

#[test]
fn test_resolve_non_commit_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        let rev = rev!("ebc3780a093cbda629d531c1c0d530a82063ee6f");
        let oid = repo.fully_resolve_rev_to_any(&rev)?;
        assert_eq!(oid, "ebc3780a093cbda629d531c1c0d530a82063ee6f".into());
        Ok(())
    })
}

#[test]
fn test_resolve_partial_revspec() -> BitResult<()> {
    BitRepo::find(repos_dir!("ribble"), |repo| {
        // this is a tree oid
        let rev = rev!("ebc3780");
        let oid = repo.fully_resolve_rev_to_any(&rev)?;
        assert_eq!(oid, "ebc3780a093cbda629d531c1c0d530a82063ee6f".into());
        Ok(())
    })
}

#[test]
fn test_fully_resolve_revspec_non_existent_ancestor() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let rev = rev!("HEAD~2000");
        let err = repo.fully_resolve_rev(&rev).unwrap_err();
        assert_eq!(
            err.to_string(),
            "revision `HEAD~2000` refers to the parent of an initial commit"
        );
        Ok(())
    })
}
