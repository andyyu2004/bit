use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;

macro_rules! diff_unstaged {
    ($repo:ident) => {{
        let diff = $repo.diff_index_worktree(Pathspec::MATCH_ALL)?;
        let mut output = vec![];
        diff.format_into($repo, &mut output)?;
        String::from_utf8(output).unwrap()
    }};
}

#[test]
fn test_diff_format_unstaged_modifications() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        modify!(repo: "foo" < "some modified content\n");

        let output = diff_unstaged!(repo);

        let mut lines = output.lines();
        assert_eq!(lines.next().unwrap(), "diff --git a/foo b/foo");
        // ensure the second hash is not unknown as this file is unstaged we wouldn't
        // know it's hash as it's not added/committed
        assert_eq!(lines.next().unwrap(), "index e69de29..9122a9c 100644");

        Ok(())
    })
}

macro_rules! diff_staged {
    ($repo:ident) => {{
        let diff = $repo.diff_head_index(Pathspec::MATCH_ALL)?;
        let mut output = vec![];
        diff.format_into($repo, &mut output)?;
        String::from_utf8(output).unwrap()
    }};
}

#[test]
fn test_diff_format_deleted_staged_header() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "foo");
        modify!(repo: "foo" < "some content\n");
        bit_commit_all!(repo);

        rm!(repo: "foo");
        bit_add_all!(repo);

        let output = diff_staged!(repo);

        let mut lines = output.lines();
        assert_eq!(lines.next().unwrap(), "diff --git a/foo b/foo");
        assert_eq!(lines.next().unwrap(), "deleted file mode 100644");
        assert_eq!(lines.next().unwrap(), "index 2ef267e..0000000");
        assert_eq!(lines.next().unwrap(), "--- a/foo");
        assert_eq!(lines.next().unwrap(), "+++ /dev/null");

        Ok(())
    })
}

#[test]
fn test_diff_format_modified_staged_header() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "foo");
        modify!(repo: "foo" < "some content\n");
        bit_commit_all!(repo);

        rm!(repo: "foo");
        bit_add_all!(repo);

        let output = diff_staged!(repo);

        let mut lines = output.lines();
        assert_eq!(lines.next().unwrap(), "diff --git a/foo b/foo");
        assert_eq!(lines.next().unwrap(), "deleted file mode 100644");
        assert_eq!(lines.next().unwrap(), "index 2ef267e..0000000");
        assert_eq!(lines.next().unwrap(), "--- a/foo");
        assert_eq!(lines.next().unwrap(), "+++ /dev/null");

        Ok(())
    })
}

#[test]
fn test_diff_format_created_staged_header() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        touch!(repo: "new");
        bit_add_all!(repo);

        let output = diff_staged!(repo);

        let mut lines = output.lines();
        assert_eq!(lines.next().unwrap(), "diff --git a/new b/new");
        assert_eq!(lines.next().unwrap(), "new file mode 100644");
        assert_eq!(lines.next().unwrap(), "index 0000000..e69de29");
        assert_eq!(lines.next().unwrap(), "--- /dev/null");
        assert_eq!(lines.next().unwrap(), "+++ b/new");

        Ok(())
    })
}
