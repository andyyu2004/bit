use crate::error::BitResult;
use crate::obj::FileMode;
use crate::repo::BitRepo;

#[test]
fn test_simple_checkout_rm_rf() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        repo.checkout(&rev!("HEAD^"))?;

        assert!(repo.read_head()?.is_direct());

        let mut iter = repo.with_index(|index| index.worktree_iter())?;
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.next() => "foo":FileMode::REG);
        Ok(())
    })
}

#[test]
fn test_checkout_moves_head_to_branch_not_commit() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        // HEAD should resolve to a branch
        repo.checkout(&rev!("HEAD"))?;
        assert!(repo.read_head()?.is_symbolic());

        // however, HEAD^ resolves to a commit and so should move head to be direct (detached head)
        repo.checkout(&rev!("HEAD^"))?;
        assert!(repo.is_head_detached()?);

        repo.checkout(&rev!("master"))?;
        let head = repo.read_head()?;
        assert!(head.is_symbolic());
        assert_eq!(head, symbolic_ref!("master"));

        Ok(())
    })
}