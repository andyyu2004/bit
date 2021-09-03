use crate::error::{BitErrorExt, BitResult};
use crate::obj::FileMode;
use crate::refs::BitRef;
use crate::repo::BitRepo;

#[test]
fn test_simple_checkout_rm_rf() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_checkout!(repo: "HEAD^")?;

        assert!(repo.read_head()?.is_direct());

        let mut iter = repo.with_index(|index| index.worktree_iter())?;
        check_next!(iter.next() => "bar":FileMode::REG);
        check_next!(iter.next() => "foo":FileMode::REG);
        Ok(())
    })
}

#[test]
fn test_checkout_remote_branch_leads_to_detached_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        repo.create_branch(symbolic!("refs/remotes/origin/master"), HEAD!())?;
        bit_checkout!(repo: "origin/master")?;
        assert!(repo.is_head_detached()?);
        Ok(())
    })
}

#[test]
fn test_checkout_moves_head_to_branch_not_commit() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        // HEAD should resolve to a branch
        bit_checkout!(repo: "HEAD")?;
        assert!(repo.read_head()?.is_symbolic());

        // however, HEAD^ resolves to a commit and so should move head to be direct (detached head)
        bit_checkout!(repo: "HEAD^")?;
        assert!(repo.is_head_detached()?);
        assert_eq!(
            repo.read_head()?,
            BitRef::Direct("6b5041d58b7ac78bad7be3b727ba605a82a94b25".into())
        );

        bit_checkout!(repo: "master")?;
        let head = repo.read_head()?;
        assert!(head.is_symbolic());
        // the symbolic reference should be expanded
        assert_eq!(head, symbolic_ref!("refs/heads/master"));

        Ok(())
    })
}

// case 1
#[test]
fn test_safe_checkout_keeps_untracked() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "untracked");
        mkdir!(repo: "new-dir");
        touch!(repo: "new-dir/bar");
        bit_checkout!(repo: "master")?;
        assert!(exists!(repo: "untracked"));
        assert!(exists!(repo: "new-dir/bar"));
        Ok(())
    })
}

// case 2
#[test]
fn test_force_checkout_removes_untracked() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "untracked");
        mkdir!(repo: "new-dir");
        touch!(repo: "new-dir/bar");
        bit_checkout!(repo: --force "master")?;
        assert!(!exists!(repo: "untracked"));
        assert!(!exists!(repo: "new-dir"));
        assert!(!exists!(repo: "new-dir/bar"));
        Ok(())
    })
}

// case 3 (safe)
#[test]
fn test_safe_checkout_of_independentally_added_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        // essentially emulating an addition in the target by removing the file and committing
        // and then trying to go back
        rm!(repo: "foo");
        bit_commit_all!(repo);
        assert!(!exists!(repo: "foo"));

        // then we create a matching file in the worktree
        touch!(repo: "foo" < "default foo contents");
        // To be honest, I don't know why this generates a conflict when both the new files are the same.
        // But both git and libgit2 have this behaviour so we shall match it
        let conflicts = bit_checkout!(repo: "HEAD^").unwrap_err().try_into_checkout_conflict()?;
        // TODO add more assertions once the conflict type is more developed
        assert_eq!(conflicts.worktree.len(), 1);
        Ok(())
    })
}

// case 3 (forced)
#[test]
fn test_force_checkout_of_independentally_added_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_commit_all!(repo);

        touch!(repo: "foo" < "default foo contents");
        bit_checkout!(repo: --force "HEAD^")?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        Ok(())
    })
}

// case 4 (safe)
#[test]
fn test_safe_checkout_of_added_blob_with_content_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_commit_all!(repo);

        // then we create a conflicting file in the worktree
        touch!(repo: "foo" < "conflicting foo contents");
        let conflicts = bit_checkout!(repo: "HEAD^").unwrap_err().try_into_checkout_conflict()?;
        // TODO add more assertions once the conflict type is more developed
        assert_eq!(conflicts.worktree.len(), 1);
        Ok(())
    })
}

// case 4 (forced)
#[test]
fn test_forced_checkout_of_added_blob_with_content_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_commit_all!(repo);

        touch!(repo: "foo" < "new foo contents");
        bit_checkout!(repo: --force "HEAD^")?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        Ok(())
    })
}

// case 5
#[test]
fn test_checkout_add_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "default foo contents"
            newdir {
                bar {
                    a < "a"
                }
                b < "b"
            }
        };

        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        assert!(exists!(repo: "newdir"));
        assert!(exists!(repo: "newdir/bar"));
        assert_eq!(cat!(repo: "newdir/bar/a"), "a");
        assert_eq!(cat!(repo: "newdir/b"), "b");
        Ok(())
    })
}

// case 6
#[test]
fn test_checkout_add_tree_with_blob_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = tree! {
            foo < "default foo contents"
            new {
                bar
            }
        };

        touch!(repo: "new");
        // bit_checkout!(repo: &rev!(target))?;

        Ok(())
    })
}

// case 15
#[test]
fn test_safe_checkout_locally_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        touch!(repo: "foo" < "new foo contents");
        bit_checkout!(repo: "master")?;
        // `foo` should have the workdir contents as base and target match
        assert_eq!(cat!(repo: "foo"), "new foo contents");
        Ok(())
    })
}

// case 15 (forced)
#[test]
fn test_force_checkout_locally_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        touch!(repo: "foo" < "new foo contents");
        bit_checkout!(repo: --force "master")?;
        // force checkout should reset the working directory
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        Ok(())
    })
}
