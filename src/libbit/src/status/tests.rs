use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_status_untracked_files() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo, "foo");
        touch!(repo, "bar");
        touch!(repo, "baz");
        bit_add!(repo, "bar");

        let untracked = repo.untracked_files()?;
        assert_eq!(untracked.len(), 2);
        assert_eq!(untracked[0], "baz");
        assert_eq!(untracked[1], "foo");
        Ok(())
    })
}
