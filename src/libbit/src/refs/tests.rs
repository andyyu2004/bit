use crate::error::BitResult;
use crate::repo::BitRepo;

use super::is_valid_name;

#[test]
fn test_resolve_symref_that_points_to_nonexistent_file() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        // repo initializes with `HEAD` pointing to `refs/heads/master`
        // resolving nonexistent symbolic ref should just return itself (minus the prefix)
        assert_eq!(
            repo.resolve_ref(symbolic_ref!("ref: refs/heads/master"))?,
            symbolic_ref!("refs/heads/master"),
        );
        Ok(())
    })
}

#[test]
fn test_resolve_head_symref() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        assert_eq!(repo.resolve_ref(HEAD!())?, symbolic_ref!("refs/heads/master"));
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
