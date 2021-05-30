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

macro_rules! bit_commit {
    ($repo:expr) => {
        $repo.commit(Some(String::from("arbitrary message")))?;
    };
}

macro_rules! bit_commit_all {
    ($repo:expr) => {{
        bit_add_all!($repo);
        bit_commit!($repo)
    }};
}

macro_rules! bit_add_all {
    ($repo:expr) => {
        $repo.bit_add_all()?
    };
}

macro_rules! bit_status {
    ($repo:expr) => {
        $repo.status(crate::pathspec::Pathspec::MATCH_ALL)?
    };
    ($repo:ident in $pathspec:expr) => {
        $repo.status_report($pathspec)?
    };
}

macro_rules! mkdir {
    ($repo:ident: $path:expr) => {
        std::fs::create_dir($repo.workdir.join($path))?
    };
}

macro_rules! bit_add {
    ($repo:ident: $pathspec:expr) => {
        $repo.index_add($pathspec)?
    };
}

macro_rules! touch {
    ($repo:ident: $path:expr) => {
        std::fs::File::create($repo.workdir.join($path))?
    };
}

macro_rules! symlink {
    ($repo:ident: $original:literal <- $link:literal) => {
        let original = $repo.workdir.join($original);
        let link = $repo.workdir.join($link);
        std::os::unix::fs::symlink(original, link)?
    };
}

macro_rules! random {
    () => {
        crate::test_utils::generate_sane_string(50..1000)
    };
}

macro_rules! stat {
    ($repo:ident: $path:literal) => {
        #[allow(unused_imports)]
        use std::os::linux::fs::*;
        let metadata = std::fs::symlink_metadata($repo.workdir.join($path))?;
        eprintln!(
            "ctime {}:{}; mtime: {} {}; size: {}",
            metadata.st_ctime(),
            metadata.st_ctime_nsec(),
            metadata.st_mtime(),
            metadata.st_mtime_nsec() as u32,
            metadata.st_size()
        );
    };
}

macro_rules! modify {
    ($repo:ident: $path:literal < $content:expr) => {
        #[allow(unused_imports)]
        use std::io::prelude::*;
        let mut file = std::fs::File::with_options()
            .read(false)
            .append(false)
            .write(true)
            .open($repo.workdir.join($path))?;
        file.write_all($content.as_ref())?;
        file.sync_all()?
    };
    ($repo:ident: $path:literal << $content:expr) => {
        #[allow(unused_imports)]
        use std::io::prelude::*;
        let mut file = std::fs::File::with_options()
            .read(false)
            .append(true)
            .open($repo.workdir.join($path))?;
        file.write_all($content.as_ref())?;
        file.sync_all()?;
    };
    ($repo:ident: $path:literal) => {
        #[allow(unused_imports)]
        use std::io::prelude::*;
        let mut file = std::fs::File::with_options()
            .read(false)
            .write(true)
            .open($repo.workdir.join($path))?;
        file.write_all(random!().as_bytes())?;
        file.sync_all()?;
    };
}

macro_rules! readlink {
    ($repo:ident: $path:expr) => {
        std::fs::read_link($repo.workdir.join($path))?
    };
}

macro_rules! hash_symlink {
    ($repo:ident: $path:expr) => {{
        let path = readlink!($repo: $path);
        hash_blob!(path.to_str().unwrap().as_bytes())
    }};
}

macro_rules! hash_blob {
    ($bytes:expr) => {
        crate::hash::hash_obj(&Blob::new($bytes.to_vec()))?;
    };
}
macro_rules! hash_file {
    ($repo:ident: $path:expr) => {
        hash_blob!(cat!($repo: $path).as_bytes())
    };
}

macro_rules! cat {
    ($repo:ident: $path:expr) => {
        std::fs::read_to_string($repo.workdir.join($path))?
    };
}

/// `rm -r`
macro_rules! rmdir {
    ($repo:ident: $path:expr) => {
        std::fs::remove_dir_all($repo.workdir.join($path))?
    };
}

macro_rules! rm {
    ($repo:ident: $path:expr) => {
        std::fs::remove_file($repo.workdir.join($path))?
    };
}

// absolute path to the tests directory
macro_rules! tests_dir {
    () => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
    };
    ($path:expr) => {
        tests_dir!().join($path)
    };
}

macro_rules! repos_dir {
    () => {
        tests_dir!("repos")
    };
    ($path:expr) => {
        repos_dir!().join($path)
    };
}

macro_rules! symbolic {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::SymbolicRef::from_str($sym).unwrap()
    }};
}

macro_rules! symbolic_ref {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::BitRef::Symbolic(symbolic!($sym))
    }};
}

macro_rules! HEAD {
    () => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::BitRef::Symbolic(crate::refs::SymbolicRef::from_str("HEAD").unwrap())
    }};
}

macro_rules! parse_rev {
    ($rev:expr) => {{
        // NOTE: `eval` must be called with a repository in scope (tls)
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::rev::LazyRevspec::from_str($rev)?
    }};
}
