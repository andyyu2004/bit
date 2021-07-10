use crate::error::BitResult;
use crate::obj::FileMode;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;

#[test]
fn test_diff_two_same_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let oid = repo.head_tree()?;
        // TODO test performance and number of comparisons etc
        let diff = repo.diff_tree_to_tree(oid, oid)?;
        assert!(diff.is_empty());
        Ok(())
    })
}

#[test]
fn test_diff_head_prime_to_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let head_prime = repo.fully_resolve_rev(&rev!("HEAD^"))?;
        let oid = repo.read_obj(head_prime)?.into_commit().tree;

        let diff = repo.diff_tree_to_tree(oid, repo.head_tree()?)?;
        assert!(diff.modified.is_empty());
        assert!(diff.deleted.is_empty());
        assert_eq!(diff.new.len(), 4);

        // so the iterator should expand the new directory
        // and not just add the directory itself
        assert_eq!(diff.new[0].path, "dir/bar.l");
        assert_eq!(diff.new[0].mode, FileMode::REG);
        assert_eq!(diff.new[1].path, "dir/bar/qux");
        assert_eq!(diff.new[1].mode, FileMode::REG);
        assert_eq!(diff.new[2].path, "dir/baz");
        assert_eq!(diff.new[2].mode, FileMode::REG);
        assert_eq!(diff.new[3].path, "dir/link");
        assert_eq!(diff.new[3].mode, FileMode::LINK);
        Ok(())
    })
}

#[test]
fn test_diff_tree_to_tree_deleted() -> BitResult<()> {
    // TODO test fails
    BitRepo::with_empty_repo(|repo| {
        let a = tree! {
            bar
            foo {
                a
                b
            }
            qux
        };

        let b = tree! {
            bar
            qux
        };

        let diff = repo.diff_tree_to_tree(a, b)?;
        assert!(diff.new.is_empty());
        assert!(diff.modified.is_empty());
        assert_eq!(diff.deleted.len(), 2);
        Ok(())
    })
}

// check empty non existent head is considered an empty tree/iterator
#[test]
fn test_diff_no_head_with_index() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "foo");
        bit_add_all!(repo);
        let diff = repo.diff_ref_index(BitRef::HEAD, Pathspec::MATCH_ALL)?;
        assert!(diff.deleted.is_empty());
        assert!(diff.modified.is_empty());
        assert_eq!(diff.new.len(), 1);
        Ok(())
    })
}

// expected output from `bit status`:
// modified:   logic-ir/src/tls.rs
#[test]
fn test_diff_head_index_on_logic_repo() -> BitResult<()> {
    BitRepo::find(repos_dir!("logic"), |repo| {
        let pathspec = "logic-ir".parse()?;
        let diff = repo.diff_head_index(pathspec)?;
        assert!(diff.new.is_empty());
        assert!(diff.deleted.is_empty());
        assert_eq!(diff.modified.len(), 1);

        Ok(())
    })
}

#[test]
fn test_tree_diff_replace_dir_with_file() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let a = tree! {
            foo {
                a
                b
            }
        };
        let b = tree! {
            foo
        };
        let diff = repo.diff_tree_to_tree(a, b)?;
        assert!(diff.modified.is_empty());

        assert_eq!(diff.new.len(), 1);
        assert_eq!(diff.new[0].path, "foo");

        assert_eq!(diff.deleted.len(), 2);
        assert_eq!(diff.deleted[0].path, "foo/a");
        assert_eq!(diff.deleted[1].path, "foo/b");
        Ok(())
    })
}

#[test]
fn test_tree_diff_replace_file_with_dir() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let a = tree! {
            x
            a
            foo
        };

        let b = tree! {
            a
            x
            foo {
                a
                b
            }
        };

        let diff = repo.diff_tree_to_tree(a, b)?;
        assert!(diff.modified.is_empty());

        assert_eq!(diff.deleted.len(), 1);
        assert_eq!(diff.deleted[0].path, "foo");

        assert_eq!(diff.new.len(), 2);
        assert_eq!(diff.new[0].path, "foo/a");
        assert_eq!(diff.new[1].path, "foo/b");
        Ok(())
    })
}
