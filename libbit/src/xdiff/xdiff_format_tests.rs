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

        assert_eq!(lines.next().unwrap(), "diff --git a/foo b/foo");
        assert_eq!(lines.next().unwrap(), "diff --git a/foo b/foo");
        assert_eq!(lines.next().unwrap(), "diff --git a/new b/new");