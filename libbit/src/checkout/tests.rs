use crate::error::{BitErrorExt, BitResult};
use crate::obj::FileMode;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;

// TODO all cases where workdir has a Tree are unimplemented/untested

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

// case 1 (safe)
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

// case 1 (forced)
#[test_env_log::test]
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

// case 2
#[test]
fn test_checkout_add_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "default foo contents"
            newfile
        };
        bit_checkout!(repo: &rev!(target))?;
        assert!(exists!(repo: "newfile"));

        bit_reset!(repo: --hard "master");
        assert!(!exists!(repo: "newfile"));

        bit_checkout!(repo: &rev!(target))?;
        assert!(exists!(repo: "newfile"));
        Ok(())
    })
}

// case 3 (safe)
#[test]
fn test_safe_checkout_of_independently_added_blob() -> BitResult<()> {
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
fn test_force_checkout_of_independently_added_blob() -> BitResult<()> {
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

// case 6 (safe)
#[test]
fn test_safe_checkout_add_tree_with_blob_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "default foo contents"
            new {
                bar
                nested {
                    c
                    d
                    e
                }
            }
        };

        touch!(repo: "new");
        let conflicts =
            bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        assert_eq!(conflicts.len(), 1);
        Ok(())
    })
}

// case 6 (forced)
#[test]
fn test_forced_checkout_add_tree_with_blob_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "default foo contents"
            new {
                bar {
                    nested {
                        c
                        d
                        e
                    }
                    z
                }
            }
        };

        touch!(repo: "new");
        bit_checkout!(repo: --force &rev!(target))?;
        assert!(exists!(repo: "new/bar/z"));
        assert!(exists!(repo: "new/bar/nested/d"));
        Ok(())
    })
}

// case 7 (safe)
#[test_env_log::test]
fn test_safe_checkout_independently_added_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            dir {
                bar
                baz
                nested {
                    foo
                }
            }
        };
        mkdir!(repo: "dir");
        touch!(repo: "dir/bar");
        touch!(repo: "dir/baz");
        mkdir!(repo: "dir/nested");
        touch!(repo: "dir/nested/notfoo");
        // unsure what the correct semantics is, but just conflicting for now is an easy solution :)
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 7 (forced)
#[test_env_log::test]
fn test_force_checkout_independently_added_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            dir {
                bar
                baz
                nested {
                    foo
                }
            }
        };
        mkdir!(repo: "dir");
        touch!(repo: "dir/bar");
        touch!(repo: "dir/baz");
        mkdir!(repo: "dir/nested");
        touch!(repo: "dir/nested/notfoo");
        bit_checkout!(repo: --force &rev!(target))?;
        assert!(exists!(repo: "dir/nested/foo"));
        assert!(!exists!(repo: "dir/nested/notfoo"));
        Ok(())
    })
}

// case 8 (safe)
#[test]
fn test_safe_checkout_independently_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_checkout!(repo: &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

// case 8 (forced)
#[test]
fn test_force_checkout_independently_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_checkout!(repo: --force &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

// case 9
#[test]
fn test_checkout_delete_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        bit_checkout!(repo: &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));

        bit_checkout!(repo: "master")?;
        assert!(exists!(repo: "foo"));
        bit_checkout!(repo: --force &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

// case 10 (safe)
#[test]
fn test_safe_checkout_delete_of_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        modify!(repo: "foo");
        let conflicts =
            bit_checkout!(repo: &rev!(commit! {})).unwrap_err().try_into_checkout_conflict()?;
        assert_eq!(conflicts.len(), 1);
        Ok(())
    })
}

// case 10 (forced)
#[test]
fn test_force_checkout_delete_of_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        modify!(repo: "foo");
        bit_checkout!(repo: --force &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

// case 11 (safe)
#[test]
fn test_safe_checkout_independently_deleted_blob_and_untracked_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: &rev!(commit! {}))?;
        assert!(exists!(repo: "foo/bar"));
        Ok(())
    })
}

// case 11 (forced)
#[test]
fn test_force_checkout_independently_deleted_blob_and_untracked_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: --force &rev!(commit! {}))?;
        assert!(!exists!(repo: "foo"));
        assert!(!exists!(repo: "foo/bar"));
        Ok(())
    })
}

// case 12 (safe)
#[test]
fn test_safe_checkout_locally_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_checkout!(repo: "master")?;
        // will leave the file as deleted
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

// case 12 (forced)
#[test]
fn test_force_checkout_locally_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        bit_checkout!(repo: --force "master")?;
        // will recreate the locally deleted blob
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        Ok(())
    })
}

// case 13 (safe)
// libgit2 table seems wrong in this case, says SAFE+MISSING?
// but in code
// `*action = CHECKOUT_ACTION_IF(RECREATE_MISSING, UPDATE_BLOB, CONFLICT);`
#[test]
fn test_safe_checkout_update_to_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
           foo < "updated content"
        };
        rm!(repo: "foo");
        let conflicts =
            bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        assert_eq!(conflicts.len(), 1);
        Ok(())
    })
}

// case 13 (forced)
#[test]
fn test_force_checkout_update_to_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
           foo < "target content"
        };
        rm!(repo: "foo");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "target content");
        Ok(())
    })
}

// case 14
#[test]
fn test_checkout_unmodified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        bit_checkout!(repo: "master")?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        bit_checkout!(repo: --force "master")?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
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

// case 16
#[test]
fn test_checkout_update_unmodified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "target content"
        };
        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "target content");
        Ok(())
    })
}

// case 17 (safe)
#[test]
fn test_safe_checkout_independently_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "new content"
        };
        modify!(repo: "foo" < "new content");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 17 (forced)
#[test]
fn test_force_checkout_independently_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "new content"
        };
        modify!(repo: "foo" < "new content");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "new content");
        Ok(())
    })
}

// case 18 (safe)
#[test]
fn test_safe_checkout_update_to_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "new content"
        };
        modify!(repo: "foo" < "differing new content");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 18 (forced)
#[test]
fn test_forced_checkout_update_to_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "new content"
        };
        modify!(repo: "foo" < "differing new content");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "new content");
        Ok(())
    })
}

// case 19 (safe)
#[test_env_log::test]
fn test_safe_checkout_local_blob_to_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: "master")?;
        assert!(exists!(repo: "foo/bar"));
        Ok(())
    })
}

// case 19 (forced)
#[test]
fn test_force_checkout_local_blob_to_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: --force "master")?;
        assert_eq!(cat!(repo: "foo"), "default foo contents");
        Ok(())
    })
}

// case 20 (safe)
#[test]
fn test_safe_checkout_updated_blob_with_untracked_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "updated foo contents"
        };
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 20 (forced)
#[test]
fn test_force_checkout_updated_blob_with_untracked_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo < "updated foo contents"
        };
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo"), "updated foo contents");
        Ok(())
    })
}

// case 21
#[test]
fn test_checkout_add_tree_with_locally_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        rm!(repo: "foo");
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

// case 22
#[test]
fn test_checkout_blob_to_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

// case 23 (safe)
#[test]
fn test_safe_checkout_blob_to_tree_with_locally_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        modify!(repo: "foo");
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 23 (forced)
#[test]
fn test_forced_checkout_blob_to_tree_with_locally_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        modify!(repo: "foo");
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

// case 24
#[test]
fn test_safe_checkout_add_tree_with_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar" < "bar contents");

        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 24 (forced)
#[test]
fn test_force_checkout_add_tree_with_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let target = commit! {
            foo {
                bar < "bar contents"
            }
        };
        rm!(repo: "foo");
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar" < "bar contents");

        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

// case 25
#[test]
fn test_checkout_independently_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
        };
        rmdir!(repo: "dir");
        bit_checkout!(repo: &rev!(target))?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

// case 26 (safe)
#[test]
fn test_safe_checkout_independently_deleted_tree_with_untracked_blob() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 26 (forced)
#[test]
fn test_force_checkout_independently_deleted_tree_with_untracked_blob() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir");
        bit_checkout!(repo: --force &rev!(target))?;
        assert!(!exists!(repo: "dir/baz"));
        assert!(!exists!(repo: "dir/bar/qux"));
        Ok(())
    })
}

// case 27
#[test]
fn test_checkout_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        bit_branch!(repo: "checkpoint");
        let target = commit! {
            foo
            bar
        };
        bit_checkout!(repo: &rev!(target))?;
        assert!(!exists!(repo: "dir"));

        // TODO fixme
        // bit_reset!(repo: --hard "checkpoint");
        // bit_checkout!(repo: --force &rev!(target))?;
        // assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

// case 28
#[test]
fn test_checkout_deleted_tree_and_added_blob() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        bit_branch!(repo: "checkpoint");
        let target = commit! {
            foo
            bar
            dir < "changed to a file"
        };
        rmdir!(repo: "dir");
        bit_checkout!(repo: &rev!(target))?;

        // reset and test force checkout as well
        bit_reset!(repo: --hard "checkpoint");
        assert!(exists!(repo: "dir/bar"));
        rmdir!(repo: "dir");
        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "dir"), "changed to a file");
        Ok(())
    })
}

// case 29 (safe)
#[test]
fn test_safe_checkout_independently_typechanged_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
            dir < "changed to a file"
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "changed to a file");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 29 (forced)
#[test]
fn test_force_checkout_independently_typechanged_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
            dir < "changed to a file"
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "changed to a file");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "dir"), "changed to a file");
        Ok(())
    })
}

// case 30 (safe)
#[test]
fn test_safe_checkout_typechange_tree_to_blob_with_conflicting() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
            dir < "changed to a file"
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "changed to a different file");
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 31 (forced)
#[test]
fn test_force_checkout_typechange_tree_to_blob_with_conflicting() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        let target = commit! {
            foo
            bar
            dir < "changed to a file original"
        };
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "changed to a different file");
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "dir"), "changed to a file original");
        Ok(())
    })
}

// case 32 (safe)
#[test]
fn test_safe_checkout_locally_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        rmdir!(repo: "dir");
        bit_checkout!(repo: "master")?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

// case 32 (forced)
#[test]
fn test_force_checkout_locally_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        rmdir!(repo: "dir");
        bit_checkout!(repo: --force "master")?;
        assert!(exists!(repo: "dir"));
        Ok(())
    })
}

// case 33 (safe)
#[test]
fn test_safe_checkout_local_tree_to_blob() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "hi");
        bit_checkout!(repo: "master")?;
        assert_eq!(cat!(repo: "dir"), "hi");
        Ok(())
    })
}

// case 33 (forced)
#[test_env_log::test]
fn test_forced_checkout_local_tree_to_blob() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        rmdir!(repo: "dir");
        touch!(repo: "dir" < "hi");
        bit_checkout!(repo: --force "master")?;
        assert!(exists!(repo: "dir/baz"));
        assert!(exists!(repo: "dir/bar.l"));
        Ok(())
    })
}

// case 34 (safe)
#[test]
fn test_safe_checkout_locally_modified_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "dir/bar.l" < "modified");
        bit_checkout!(repo: "master")?;
        assert_eq!(cat!(repo: "dir/bar.l"), "modified");
        Ok(())
    })
}

// case 34 (forced)
#[test]
fn test_forced_checkout_locally_modified_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "dir/bar.l" < "modified");
        bit_checkout!(repo: --force "master")?;
        assert_eq!(cat!(repo: "dir/bar.l"), "");
        Ok(())
    })
}

// case 35 (safe)
#[test]
fn test_safe_checkout_update_locally_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        rmdir!(repo: "dir");
        let target = commit! {
            foo
            bar
            dir {
                bar < "modified"
            }
        };
        bit_checkout!(repo: &rev!(target))?;
        assert_eq!(cat!(repo: "dir/bar"), "modified");
        Ok(())
    })
}

// case 35 (forced)
#[test]
fn test_force_checkout_update_locally_deleted_tree() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        rmdir!(repo: "dir");
        let target = commit! {
            foo
            bar
            dir {
                bar < "modified"
            }
        };
        bit_checkout!(repo: --force &rev!(target))?;
        assert_eq!(cat!(repo: "dir/bar"), "modified");
        Ok(())
    })
}

// case 36 (safe)
#[test_env_log::test]
fn test_safe_checkout_updated_tree_with_local_tree_to_blob_conflict() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        rmdir!(repo: "dir");
        touch!(repo: "dir");
        let target = commit! {
            foo
            bar
            dir {
                bar < "modified"
            }
        };
        bit_checkout!(repo: &rev!(target)).unwrap_err().try_into_checkout_conflict()?;
        Ok(())
    })
}

// case 37 (safe)
#[test_env_log::test]
fn test_force_checkout_updated_tree_with_local_tree_to_blob_conflict() -> BitResult<()> {
    BitRepo::with_sample_repo_no_sym(|repo| {
        rmdir!(repo: "dir");
        touch!(repo: "dir");
        let target = commit! {
            foo
            bar
            dir {
                bar < "modified"
            }
        };
        bit_checkout!(repo: --force &rev!(target))?;
        assert!(repo.diff_tree_worktree(target, Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    })
}
