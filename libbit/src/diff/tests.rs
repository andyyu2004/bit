use crate::error::BitResult;
use crate::obj::FileMode;
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
fn test_diff_tree_to_tree() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let head = repo.resolve_rev(&parse_rev!("HEAD^"))?;
        let oid = repo.read_obj(head)?.into_commit().tree;

        let diff = repo.diff_tree_to_tree(oid, repo.head_tree_oid()?)?;
        dbg!(&diff.new.iter().map(|entry| entry.path).collect::<Vec<_>>());
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
