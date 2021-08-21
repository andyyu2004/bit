use crate::bit;

#[test]
fn test_cli_switch_on_empty_repo() {
    bit!("-C tests/repos/empty switch -c some-new-branch")
        .stdout("switched to a new branch `some-new-branch`\n");
}
