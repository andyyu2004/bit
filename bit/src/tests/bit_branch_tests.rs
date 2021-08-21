#[test]
fn test_cli_branch_on_empty_repo() {
    bit!("-C tests/repos/empty branch -c some-new-branch")
        .stdout( "cannot create new branch on an empty branch (use `bit switch -c <branch>` to change your branch)");
}
