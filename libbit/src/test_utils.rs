use crate::error::BitResult;
use crate::obj::{BitObjKind, BitObject, FileMode, Oid, TreeEntry};
use crate::path::BitPath;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use rand::Rng;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum DebugTreeEntry {
    Tree(DebugTree),
    File(TreeEntry),
}

impl DebugTreeEntry {
    pub fn path(&self) -> BitPath {
        match self {
            DebugTreeEntry::Tree(tree) => tree.path,
            DebugTreeEntry::File(file) => file.path,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct DebugTree {
    path: BitPath,
    oid: Oid,
    entries: Vec<DebugTreeEntry>,
}

macro_rules! indent {
    ($f:expr, $indents:expr) => {
        write!($f, "{} ", (0..$indents).map(|_| "   ").fold(String::new(), |acc, x| acc + x))?
    };
}

impl Display for DebugTreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        indent!(f, self.path().components().len());
        match self {
            DebugTreeEntry::Tree(tree) => write!(f, "{}", tree),
            DebugTreeEntry::File(entry) =>
                writeln!(f, "{} ({})", entry.path.file_name(), entry.oid),
        }
    }
}

impl Display for DebugTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}/ ({})", self.path.file_name(), self.oid)?;
        for entry in &self.entries {
            write!(f, "{}", entry)?;
        }
        Ok(())
    }
}

impl<'a> From<&'a str> for BitRef {
    fn from(s: &'a str) -> Self {
        Self::from_str(s).unwrap()
    }
}

impl BitRepo<'_> {
    // get a view of the entire recursive tree structure
    pub fn debug_tree(self, oid: Oid) -> BitResult<DebugTree> {
        self.debug_tree_internal(oid, BitPath::EMPTY)
    }

    fn debug_tree_internal(self, oid: Oid, path: BitPath) -> BitResult<DebugTree> {
        let tree = self.read_obj_tree(oid)?;
        let mut entries = Vec::with_capacity(tree.entries.len());
        for &entry in &tree.entries {
            let path = path.join(entry.path);
            let entry = TreeEntry { path, ..entry };
            let dbg_entry = match self.read_obj(entry.oid)? {
                BitObjKind::Blob(..) => DebugTreeEntry::File(entry),
                BitObjKind::Tree(tree) =>
                    DebugTreeEntry::Tree(self.debug_tree_internal(tree.oid(), path)?),
                _ => unreachable!(),
            };
            entries.push(dbg_entry);
        }

        Ok(DebugTree { path, entries, oid: tree.oid() })
    }
}
pub fn generate_random_string(range: std::ops::Range<usize>) -> String {
    let size = rand::thread_rng().gen_range(range);
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(size)
        .map(char::from)
        .collect()
}

// String::arbitrary is not so good sometimes as it doesn't generate printable strings
// not ideal as it doesn't generate '\n',' ','/' and other valid characters
// does some really arbitrary crap logic but should be fine
pub fn generate_sane_string_with_newlines(range: std::ops::Range<usize>) -> String {
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

macro_rules! check_next {
    ($next:expr => $path:literal:$mode:expr) => {
        #[allow(unused_imports)]
        use crate::iter::*;
        let entry = $next?.unwrap();
        assert_eq!(entry.path(), $path);
        assert_eq!(entry.mode(), $mode);
    };
}

macro_rules! exists {
    ($repo:ident: $path:literal) => {
        $repo.workdir.join($path).exists()
    };
}

macro_rules! test_serde {
    ($item:ident) => {{
        let mut buf = vec![];
        $item.serialize(&mut buf)?;
        assert_eq!($item, Deserialize::deserialize_unbuffered(&buf[..])?);
        Ok(())
    }};
}
macro_rules! bit_commit {
    ($repo:expr) => {
        $repo.commit(Some(String::from("arbitrary message")))?
    };
}

macro_rules! bit_checkout {
    ($repo:ident: $rev:literal) => {{
        let revision = $rev.parse::<$crate::rev::Revspec>()?;
        $repo.checkout_revision(&revision, Default::default())?;
    }};
    ($repo:ident: $rev:expr) => {{
        $repo.checkout_revision($rev, Default::default())?;
    }};
}

macro_rules! bit_branch {
    ($repo:ident: -b $branch:literal) => {
        $repo.bit_create_branch($branch, &rev!("HEAD"))?;
        bit_checkout!($repo: $branch)
    };
    ($repo:ident: $branch:literal @ $rev:expr) => {
        $repo.bit_create_branch($branch, &$rev)?
    };
    ($repo:ident: $branch:literal) => {
        $repo.bit_create_branch($branch, &rev!("HEAD"))?
    };
}

macro_rules! bit_reset {
    ($repo:ident: --soft $rev:expr) => {{
        let revision = $rev.to_string().parse::<$crate::rev::Revspec>()?;
        $repo.reset(&revision, $crate::reset::ResetKind::Soft)?;
    }};
    ($repo:ident: --hard $rev:expr) => {{
        let revision = $rev.to_string().parse::<$crate::rev::Revspec>()?;
        $repo.reset(&revision, $crate::reset::ResetKind::Hard)?;
    }};
    ($repo:ident: $rev:expr) => {{
        let revision = $rev.to_string().parse::<$crate::rev::Revspec>()?;
        $repo.reset(&revision, $crate::reset::ResetKind::Mixed)?;
    }};
}

macro_rules! bit_commit_all {
    ($repo:expr) => {{
        bit_add_all!($repo);
        bit_commit!($repo)
    }};
}

macro_rules! bit_merge {
    ($repo:ident: $rev:expr) => {{
        let revision = $rev.to_string().parse::<$crate::rev::Revspec>()?;
        $repo.merge(&revision).unwrap()
    }};
}

macro_rules! bit_merge_expect_conflicts {
    ($repo:ident: $rev:expr) => {{
        use crate::error::*;
        let revision = $rev.to_string().parse::<$crate::rev::Revspec>()?;
        $repo.merge(&revision).unwrap_err().try_into_merge_conflict().unwrap()
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

macro_rules! gitignore {
    ($repo:ident: { $($glob:literal)* }) => {{
        // obviously very inefficient way to write to a file but should be fine for small tests
        touch!($repo: ".gitignore");
        $({
            modify!($repo: ".gitignore" << $glob);
            modify!($repo: ".gitignore" << "\n");
        })*
    }};
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
        crate::test_utils::generate_sane_string_with_newlines(50..1000)
    };
}

macro_rules! modify {
    ($repo:ident: $path:literal < $content:expr) => {
        #[allow(unused_imports)]
        use std::io::prelude::*;
        let mut file = std::fs::File::with_options()
            .create(false)
            .read(false)
            .append(false)
            .write(true)
            .open($repo.workdir.join($path))?;
        file.write_all($content.as_ref())?;
        file.sync_all()?
    };
    ($repo:ident: $path:literal << $content:expr) => {{
        #[allow(unused_imports)]
        use std::io::prelude::*;
        let mut file = std::fs::File::with_options()
            .read(false)
            .append(true)
            .open($repo.workdir.join($path))?;
        file.write_all($content.as_ref())?;
        file.sync_all()?;
    }};
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
        let bytes = path.to_str().unwrap().as_bytes();
        // needs the obj header which is why we wrap it in blob
        MutableBlob::new(bytes.to_vec()).hash()?
    }};
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
    () => {{ tests_dir!("repos") }};
    ($path:expr) => {{
        struct DropPath(std::path::PathBuf);

        impl AsRef<std::path::Path> for DropPath {
            fn as_ref(&self) -> &std::path::Path {
                &self
            }
        }

        impl std::ops::Deref for DropPath {
            type Target = std::path::Path;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl Drop for DropPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        // We copy the entire repository to another location as otherwise we get race conditions
        // as the tests are multithreaded.
        // Its also good to not have accidental mutations to the repository data
        // The directory will be deleted after the test using `DropPath` above
        let path = repos_dir!().join($path);
        let tmpdir = tempfile::tempdir().expect("failed to get tempdir");

        fs_extra::dir::copy(path, &tmpdir, &fs_extra::dir::CopyOptions::default())
            .expect("repo copy failed");
        DropPath(tmpdir.into_path().join($path))
    }};
}

macro_rules! symbolic {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        $crate::refs::SymbolicRef::from_str($sym).unwrap()
    }};
}

macro_rules! symbolic_ref {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        $crate::refs::BitRef::Symbolic(symbolic!($sym))
    }};
}

macro_rules! HEAD {
    () => {
        &rev!("HEAD")
    };
}

#[macro_export]
macro_rules! rev {
    ($rev:literal) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        $crate::rev::Revspec::from_str($rev)?
    }};
    ($rev:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        $crate::rev::Revspec::from_str(&$rev.to_string())?
    }};
}

#[macro_export]
macro_rules! pathspec {
    ($pathspec:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        $crate::pathspec::Pathspec::from_str($pathspec)?
    }};
}

macro_rules! file_entry {
    ($path:ident < $content:literal) => {{
        let oid = $crate::tls::with_repo(|repo| {
            repo.write_obj(&$crate::obj::MutableBlob::new($content.as_bytes().to_vec()))
        })
        .unwrap();
        crate::obj::TreeEntry {
            oid,
            path: stringify!($path).into(),
            mode: $crate::obj::FileMode::REG,
        }
    }};
    ($path:literal < $content:literal) => {{
        let oid = crate::tls::with_repo(|repo| {
            repo.write_obj(&$crate::obj::MutableBlob::new($content.as_bytes().to_vec()))
        })
        .unwrap();
        crate::obj::TreeEntry { oid, path: $path.into(), mode: $crate::obj::FileMode::REG }
    }};
    ($path:expr) => {{
        let oid =
            crate::tls::with_repo(|repo| repo.write_obj(&$crate::obj::MutableBlob::new(vec![])))
                .unwrap();
        crate::obj::TreeEntry { oid, path: $path.into(), mode: $crate::obj::FileMode::REG }
    }};
}

macro_rules! dir_entry {
    ($path:expr, $tree:expr) => {{
        let oid = crate::tls::with_repo(|repo| repo.write_obj(&$tree)).unwrap();
        crate::obj::TreeEntry { oid, path: $path.into(), mode: crate::obj::FileMode::TREE }
    }};
}

macro_rules! tree_entry {
    ($path:ident) => {
        file_entry!(stringify!($path))
    };
    ($path:literal) => {
        file_entry!($path)
    };
    ($path:ident < $content:literal) => {
        file_entry!($path < $content)
    };
    ($path:literal < $content:literal) => {
        file_entry!($path < $content)
    };
    ($path:literal { $($subtree:tt)* }) => {{
        let tree = tree_obj!( $($subtree)* );
        dir_entry!($path, tree)
    }};
    ($path:ident { $($subtree:tt)* }) => {{
        let tree = tree_obj!( $($subtree)* );
        dir_entry!(stringify!($path), tree)
    }};
}

// this uses a similar technique to the json! macro where the stuff in the square brackets are entries that have been "transformed into rust" so to speak.
// they contain expressions that evaluate to tree_entries.
// the stuff on the right of the [..] is not yet translated and is just raw tokens
// we match the two cases separately
// we must match the tree case first as the other pattern will also match any tree pattern and parsing will fail
// these two cases just delegate to the `tree_entry!` macro which is fairly straightforward (the subtree case recurses back onto the `tree!` macro)
// there are cases for literals and idents and sometimes valid paths are not valid idents (but it looks nice to use idents where its legal)
macro_rules! tree_entries {
    ([ $($entries:expr,)* ] $next:ident { $($subtree:tt)* } $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next { $($subtree)* }), ] $($rest)*)
    };
    ([ $($entries:expr,)* ] $next:literal { $($subtree:tt)* } $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next { $($subtree)* }), ] $($rest)*)
    };
    ([ $($entries:expr,)* ] $next:ident < $content:literal $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next < $content), ] $($rest)*)
    };
    ([ $($entries:expr,)* ] $next:literal < $content:literal $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next < $content), ] $($rest)*)
    };
    ([ $($entries:expr,)* ] $next:ident $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next), ] $($rest)*)
    };
    ([ $($entries:expr,)* ] $next:literal $($rest:tt)*) => {
        tree_entries!([ $($entries,)* tree_entry!($next), ] $($rest)*)
    };
    ([ $($entries:expr,)* ]) => {{
        let btreeset: std::collections::BTreeSet<$crate::obj::TreeEntry> = btreeset! { $($entries,)* };
        btreeset
    }};
}

/// macro to create a `Tree`
/// uses tls to access repo, so must be used when inside a repo
/// grammar
/// <tree>       ::= { <tree-entry>* }
/// <tree-entry> ::= <path> | <path> <tree>
/// <path>       ::= <literal> | <ident>
/// note the outermost tree doesn't have explicit braces,
/// its recommended to use the `{}` delimiters for the macro invocation
/// i.e. tree! { .. } not tree! ( .. ) or tree! [ .. ]
macro_rules! tree_obj {
    ( $($entries:tt)* ) => {
        $crate::obj::MutableTree::new(tree_entries!([] $($entries)* ))
    };
}

macro_rules! p {
    ($path:expr) => {
        $crate::path::BitPath::intern($path)
    };
}

/// same as `tree_obj!` but writes it to the repo and returns the oid
macro_rules! tree {
    ( $($entries:tt)* ) => {{
        let tree = tree_obj! { $($entries)* };
        $crate::tls::with_repo(|repo| repo.write_obj(&tree)).unwrap()
    }};
}

#[test]
fn test_tree_macro() -> crate::error::BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        assert_eq!(tree_obj! {}, crate::obj::MutableTree::new(btreeset! {}));

        assert_eq!(
            tree_entries!([] foo "bar.l"),
            btreeset! {
                TreeEntry { oid: Oid::EMPTY_BLOB, path: "foo".into(), mode: FileMode::REG },
                TreeEntry { oid: Oid::EMPTY_BLOB, path: "bar.l".into(), mode: FileMode::REG },
            }
        );

        assert_eq!(
            tree_entries!([] bar { baz }),
            btreeset! {
                TreeEntry {
                    oid: "94b2978d84f4cbb7449c092255b38a1e1b40da42".into() ,
                    path: "bar".into(),
                    mode: FileMode::TREE
                },
            }
        );

        let tree = tree! {
            foo
            bar {
                baz
                "qux" {
                    quux
                }
            }
            "qux"
        };

        let debug_tree = repo.debug_tree(tree)?;
        let expected_debug_tree = DebugTree {
            path: BitPath::EMPTY,
            oid: "957fd7abc8b9f5af6700b54f6ef510017fcfe44b".into(),
            entries: vec![
                DebugTreeEntry::Tree(DebugTree {
                    path: "bar".into(),
                    oid: "de0a310f7ede65be4c9ffcdc43f7d079ce517a0e".into(),
                    entries: vec![
                        DebugTreeEntry::File(TreeEntry {
                            mode: FileMode::REG,
                            path: "bar/baz".into(),
                            oid: Oid::EMPTY_BLOB,
                        }),
                        DebugTreeEntry::Tree(DebugTree {
                            path: "bar/qux".into(),
                            oid: "c5ae2e49fc463618b26da36970bc6c662e83d9be".into(),
                            entries: vec![DebugTreeEntry::File(TreeEntry {
                                mode: FileMode::REG,
                                path: "bar/qux/quux".into(),
                                oid: Oid::EMPTY_BLOB,
                            })],
                        }),
                    ],
                }),
                DebugTreeEntry::File(TreeEntry {
                    mode: FileMode::REG,
                    path: "foo".into(),
                    oid: Oid::EMPTY_BLOB,
                }),
                DebugTreeEntry::File(TreeEntry {
                    mode: FileMode::REG,
                    path: "qux".into(),
                    oid: Oid::EMPTY_BLOB,
                }),
            ],
        };
        assert_eq!(debug_tree, expected_debug_tree);
        Ok(())
    })
}
