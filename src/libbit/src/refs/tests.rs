use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        // repo initializes with `HEAD` pointing to `refs/heads/master`
        // therefore it should resolve to None
        assert_eq!(repo.resolve_ref_opt(symbolic_ref!("ref: refs/heads/master"))?, None);
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        assert_eq!(repo.resolve_ref_opt(HEAD!())?, None);
        Ok(())
    })
}
