use fallible_iterator::FallibleIterator;

use crate::error::BitResult;
use crate::obj::{FileMode, Treeish};
use crate::repo::BitRepo;

#[test]
fn test_diff_two_same_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let tree = repo.head_tree()?;
        // TODO test performance and number of comparisons etc
        let diff = repo.diff_tree_to_tree(&tree, &tree)?;
        assert!(diff.is_empty());
        Ok(())
    })
}

#[test]
fn test_diff_two_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let oid = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let tree = repo.read_obj(oid)?.into_commit().into_tree()?;
        let head_tree = repo.head_tree()?;

        let diff = repo.diff_tree_to_tree(&tree, &head_tree)?;
        // let b = repo.tree_iter(b);
        assert!(diff.modified.is_empty());
        assert!(diff.deleted.is_empty());
        assert_eq!(diff.new.len(), 1);
        // all the files in `dir` are added between the two commits
        // so the iterator should just return `dir` as changed without recursing
        assert_eq!(diff.new[0].path, "dir");
        assert_eq!(diff.new[0].mode, FileMode::DIR);
        Ok(())
    })
}
