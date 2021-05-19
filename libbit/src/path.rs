use crate::error::BitResult;
use crate::interner::{with_path_interner, with_path_interner_mut};
use crate::io::ReadExt;
use crate::serialize::BufReadSeek;
use anyhow::Context;
use std::borrow::Borrow;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::File;
use std::io::BufReader;
use std::ops::{Deref, Index};
use std::path::{Component, Path, PathBuf};
use std::slice::SliceIndex;
use std::str::pattern::Pattern;

/// interned path (where path is just a string)
// interning paths is likely not worth it, but its nice to have it as a copy type
// since its used so much, this will also lend itself to faster comparisons as
// its now just an integer compare
#[derive(Eq, PartialEq, Clone, Copy, Hash)]
pub struct BitPath(u32);

pub type BitFileStream = impl BufReadSeek;

impl BitPath {
    pub(crate) fn new(u: u32) -> Self {
        Self(u)
    }

    pub fn empty() -> Self {
        Self::intern("")
    }

    pub fn is_empty(self) -> bool {
        self == Self::empty()
    }

    pub fn index(self) -> u32 {
        self.0
    }

    pub fn stream(self) -> BitResult<BitFileStream> {
        let file = File::open(self)
            .with_context(|| anyhow!("BitPath::stream: failed to open file `{}`", self))?;
        Ok(BufReader::new(file))
    }

    pub fn with_extension(self, ext: impl AsRef<OsStr>) -> Self {
        Self::intern(self.as_path().with_extension(ext))
    }

    pub fn join(self, path: impl AsRef<Path>) -> Self {
        Self::intern(self.as_path().join(path))
    }

    pub fn read_to_vec(self) -> BitResult<Vec<u8>> {
        if self.symlink_metadata()?.file_type().is_symlink() {
            // don't know of a better way to convert a path to bytes
            // its probably intentionally hidden
            // however, this will break for non utf8 encoded paths
            Ok(std::fs::read_link(self)?.to_str().unwrap().as_bytes().to_vec())
        } else {
            Ok(File::open(self)?.read_to_vec()?)
        }
    }

    pub fn intern_str(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        with_path_interner_mut(|interner| interner.intern_path(s))
    }

    pub fn intern(path: impl AsRef<Path>) -> Self {
        // this must be outside the `interner` closure as the `as_ref` impl may use the interner
        // leading to refcell panics
        let path = path.as_ref();
        // quite questionable turning paths into strings and then bytes
        // probably not very platform agnostic
        with_path_interner_mut(|interner| interner.intern_path(path.to_str().unwrap()))
    }

    pub fn as_str(self) -> &'static str {
        with_path_interner(|interner| interner.get_str(self))
    }

    /// returns the components of a path
    /// foo/bar/baz -> [foo, bar, baz]
    pub fn components(self) -> &'static [BitPath] {
        with_path_interner_mut(|interner| interner.get_components(self))
    }

    /// similar to `[BitPath::components](crate::path::BitPath::components)`
    /// foo/bar/baz -> [foo, foo/bar, foo/bar/baz]
    pub fn accumulative_components(self) -> impl Iterator<Item = BitPath> {
        self.components().iter().scan(BitPath::intern(""), |ps, p| {
            *ps = ps.join(p);
            Some(*ps)
        })
    }

    pub fn try_split_path_at(self, idx: usize) -> Option<(BitPath, BitPath)> {
        let components = self.components();
        if idx >= components.len() {
            return None;
        }
        let (x, y) = (&components[..idx], &components[idx]);
        let xs = x.join("/");
        Some((BitPath::intern(xs), Self::intern(&y)))
    }

    pub fn len(self) -> usize {
        self.as_str().len()
    }

    pub fn as_bytes(self) -> &'static [u8] {
        self.as_str().as_bytes()
    }

    pub fn as_path(self) -> &'static Path {
        self.as_str().as_ref()
    }

    /// returns first component of the path
    pub fn root_component(self) -> &'static Path {
        self.as_path().iter().next().unwrap().as_ref()
    }
}

impl AsRef<str> for BitPath {
    fn as_ref(&self) -> &'static str {
        self.as_str()
    }
}

impl AsRef<OsStr> for BitPath {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(self.as_str())
    }
}

impl AsRef<Path> for BitPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<'a> From<&'a Path> for BitPath {
    fn from(p: &'a Path) -> Self {
        Self::intern(p)
    }
}

impl Borrow<str> for BitPath {
    fn borrow(&self) -> &'static str {
        self.as_str()
    }
}

impl<'a> From<&'a str> for BitPath {
    fn from(s: &'a str) -> Self {
        Self::intern(s)
    }
}

impl Deref for BitPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

impl Debug for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl PartialEq<String> for BitPath {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<str> for BitPath {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for BitPath {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a> Pattern<'a> for BitPath {
    type Searcher = <&'a str as Pattern<'a>>::Searcher;

    fn into_searcher(self, haystack: &'a str) -> Self::Searcher {
        self.as_str().into_searcher(haystack)
    }
}

impl Display for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl PartialOrd for BitPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitPath {
    // from git (readcache.c)
    //     int name_compare(const char *name1, size_t len1, const char *name2, size_t len2)
    // {
    // 	size_t min_len = (len1 < len2) ? len1 : len2;
    // 	int cmp = memcmp(name1, name2, min_len);
    // 	if (cmp)
    // 		return cmp;
    // 	if (len1 < len2)
    // 		return -1;
    // 	if (len1 > len2)
    // 		return 1;
    // 	return 0;
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // files with the same subpath should come before directories
        // doesn't make sense to compare relative with absolute and vice versa
        assert_eq!(self.is_relative(), other.is_relative());
        let minlen = std::cmp::min(self.len(), other.len());
        self[..minlen].cmp(&other[..minlen]).then_with(|| self.len().cmp(&other.len()))
    }
}

impl<I> Index<I> for BitPath
where
    I: SliceIndex<str>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.as_str()[index]
    }
}

/// from cargo (https://github.com/rust-lang/cargo/blob/58a961314437258065e23cb6316dfc121d96fb71/crates/cargo-util/src/paths.rs)
/// we use this instead of canonicalize as we do NOT want symlinks to be resolved
/// Normalize a path, removing things like `.` and `..`.
///
/// CAUTION: This does not resolve symlinks (unlike
/// [`std::fs::canonicalize`]). This may cause incorrect or surprising
/// behavior at times. This should be used carefully. Unfortunately,
/// [`std::fs::canonicalize`] can be hard to use correctly, since it can often
/// fail, or on Windows returns annoying device paths. This is a problem Cargo
/// needs to improve on.
pub fn normalize(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

#[cfg(test)]
mod tests;