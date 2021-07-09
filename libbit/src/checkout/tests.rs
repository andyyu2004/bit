use crate::error::BitResult;
use crate::obj::FileMode;
use crate::peel::Peel;
use crate::repo::BitRepo;

#[test]
fn test_simple_checkout_rm_rf() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let commit = repo.resolve_rev(&rev!("HEAD^"))?.peel(repo)?;
        repo.checkout_commit(&commit)?;

        let mut iter = repo.with_index(|index| index.worktree_iter())?;
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.next() => "foo":FileMode::REG);
        Ok(())
    })
}
