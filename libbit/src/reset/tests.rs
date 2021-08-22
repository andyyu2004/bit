use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;

#[test]
fn test_reset_from_branch_to_branch() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: "b");
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/master"));
        bit_reset!(repo: "b");
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/master"));
        // `master` should NOT point at `b`, but instead to a direct reference
        assert_ne!(repo.read_ref(symbolic!("master"))?, symbolic_ref!("refs/heads/b"));
        assert!(repo.read_ref(symbolic!("master"))?.is_direct());
        Ok(())
    })
}

#[test]
fn test_simple_soft_reset() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let expected_new_head_oid = repo.fully_resolve_rev(&rev!("HEAD^"))?;
        bit_reset!(repo: --soft "HEAD^");

        assert_eq!(repo.fully_resolve_head()?, expected_new_head_oid);

        // HEAD itself should not be moved
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/master"));

        let status = repo.status(Pathspec::MATCH_ALL)?;
        // all files are soft reset should be staged as the index has not been reset
        assert!(status.unstaged.is_empty());
        // three new files have been added since the last commit
        assert_eq!(status.staged.new.len(), 3);
        assert!(status.staged.deleted.is_empty());
        assert!(status.staged.modified.is_empty());
        Ok(())
    })
}

#[test]
fn test_reset_from_detached_head_to_branch() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        // move to detached head state
        bit_checkout!(repo: "HEAD^");
        assert!(repo.is_head_detached()?);
        // resetting onto a branch should take us out of detached_head
        bit_reset!(repo: "master");
        assert!(!repo.is_head_detached()?);
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/master"));
        Ok(())
    })
}
