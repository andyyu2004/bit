use fallible_iterator::FallibleIterator;

use crate::error::BitResult;
use crate::iter::BitEntry;
use crate::obj::FileMode;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;

#[test]
fn test_diff_two_same_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let oid = repo.head_tree_oid()?;
        // TODO test performance and number of comparisons etc
        let diff = repo.diff_tree_to_tree(oid, oid)?;
        assert!(diff.is_empty());
        Ok(())
    })
}

#[test]
fn test_diff_head_prime_to_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let head_prime = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let oid = repo.read_obj(head_prime)?.into_commit().tree;

        let diff = repo.diff_tree_to_tree(oid, repo.head_tree_oid()?)?;
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
        let a = tree_oid! {
            bar
            foo {
                a
                b
            }
            qux
        };

        let b = tree_oid! {
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

#[test]
fn test_diff_head_index_todo_name() -> BitResult<()> {
    BitRepo::find(repos_dir!("logic"), |repo| {
        dbg!(repo.workdir);
        let pathspec = "logic-ir".parse()?;
        let diff = repo.diff_head_index(pathspec)?;
        dbg!(&diff);
        Ok(())
    })
}
