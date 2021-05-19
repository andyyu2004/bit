mod bit_cat_file_tests;

#[macro_export]
macro_rules! bit {
    ($args:expr) => {{
        #[allow(unused_import)]
        use assert_cmd::Command;
        Command::cargo_bin("bit").unwrap().args($args.split(' ')).assert().success()
    }};
}
