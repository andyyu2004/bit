mod bit_cat_file_tests;
mod cli_switch_tests;

#[macro_export]
macro_rules! bit {
    ($args:expr) => {{
        // this will try to compile `bit` n times (it is cached of course but there's probably a better way)
        // install a local copy of `bit` in debug (so we keep all the nice debug_assertiions)
        let status = std::process::Command::new("cargo")
            .args(&["install", "--debug", "--path", "."])
            .status()
            .expect("failed to install `bit` locally for tests");
        assert!(status.success());
        assert_cmd::Command::cargo_bin("bit").unwrap().args($args.split(' ')).assert().success()
    }};
}
