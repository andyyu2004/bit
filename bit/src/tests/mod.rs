mod bit_cat_file_tests;
mod cli_switch_tests;

#[macro_export]
macro_rules! bit {
    ($args:expr) => {{
        // this will try to compile `bit` n times (it is cached of course but there's probably a better way)
        let status = std::process::Command::new("cargo")
            .args(&["build"])
            .status()
            .expect("failed to build `bit` for tests");
        assert!(status.success());
        assert_cmd::Command::cargo_bin("bit").unwrap().args($args.split(' ')).assert().success()
    }};
}
