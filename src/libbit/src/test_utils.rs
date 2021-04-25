use rand::Rng;

// String::arbitrary is not so good sometimes as it doesn't generate printable strings
// not ideal as it doesn't generate '\n',' ','/' and other valid characters
// does some really arbitrary crap logic but should be fine
pub fn generate_sane_string(range: std::ops::Range<usize>) -> String {
    let mut newlines = rand::thread_rng().gen_range(0..10);
    let size = rand::thread_rng().gen_range(range);
    let mut s = String::new();

    loop {
        s.extend(
            rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(size / (newlines + 1))
                .map(char::from),
        );

        if newlines == 0 {
            break;
        }
        newlines -= 1;
        s.push('\n');
    }
    s
}

macro_rules! bit_add_all {
    ($repo:expr) => {
        $repo.bit_add_all()?
    };
}

macro_rules! mkdir {
    ($repo:expr, $path:expr) => {
        std::fs::create_dir($repo.workdir.join($path))?
    };
}

macro_rules! bit_add {
    ($repo:expr, $pathspec:expr) => {
        $repo.index_add($pathspec)?
    };
}
macro_rules! touch {
    ($repo:expr, $path:expr) => {
        std::fs::File::create($repo.workdir.join($path))?
    };
}

macro_rules! random {
    () => {
        crate::test_utils::generate_sane_string(50..1000)
    };
}

macro_rules! modify {
    ($repo:expr, $path:expr) => {
        #[allow(unused_imports)]
        use std::io::prelude::*;
        std::fs::File::with_options()
            .read(false)
            .write(true)
            .open($repo.workdir.join($path))?
            .write_all(random!().as_bytes())?
    };
}
macro_rules! rmdir {
    ($repo:expr, $path:expr) => {
        std::fs::remove_dir($repo.workdir.join($path))?
    };
}

macro_rules! rm {
    ($repo:expr, $path:expr) => {
        std::fs::remove_file($repo.workdir.join($path))?
    };
}
