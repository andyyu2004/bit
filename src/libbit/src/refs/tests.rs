use crate::error::BitResult;
use crate::refs::ResolvedRef;
use crate::repo::BitRepo;

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        // repo initializes with `HEAD` pointing to `refs/heads/master`
        // therefore it should resolve to None
        assert_eq!(
            repo.resolve_ref(symbolic_ref!("ref: refs/heads/master"))?,
            ResolvedRef::Partial(symbolic!("refs/heads/master")),
        );
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        assert_eq!(
            repo.resolve_ref(HEAD!())?,
            ResolvedRef::Partial(symbolic!("refs/heads/master"))
        );
        Ok(())
    })
}
