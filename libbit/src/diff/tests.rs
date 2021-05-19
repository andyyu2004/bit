use crate::error::BitResult;
use crate::obj::{FileMode, Treeish};
use crate::repo::BitRepo;

#[test]
fn test_diff_two_trees() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let oid = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let tree = repo.read_obj(oid)?.into_commit().into_tree()?;
        let head_tree = repo.head_tree()?;
        let diff = repo.diff_tree_to_tree(&tree, &head_tree)?;
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
