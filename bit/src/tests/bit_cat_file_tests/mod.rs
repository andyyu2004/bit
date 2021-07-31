use crate::bit;

#[test]
fn test_cli_cat_file_blob() {
    let expected = r#"tree 83aa323eb021e908251a5af652b1010cf1efc009
author Andy Yu <andyyu2004@gmail.com> 1621330843 +1200
committer Andy Yu <andyyu2004@gmail.com> 1621330907 +1200

init
"#;
    bit!("-C tests/repos/foo cat-file -p HEAD").stdout(expected);
}

#[test]
fn test_cli_cat_file_pretty_commit_output() {
    bit!("-C tests/repos/foo cat-file -p bc01ca0eb625f386c396605d7813be3e522df1c0")
        .stdout("qux content\n");
}

#[test]
fn test_cli_cat_file_partial_oid() {
    bit!("-C tests/repos/foo cat-file -p bc01ca0").stdout("qux content\n");
}
