use crate::error::BitResult;
use crate::interner::with_path_interner;
use crate::io::ReadExt;
use crate::serialize::Deserialize;
use std::borrow::Borrow;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// interned path (where path is just a ut8 encoded string)
// interning is not free, and should only be used if a `Copy` representation of a path is required,
// otherwise just use [std::path::Path]
#[derive(Eq, Clone, Copy)]
pub struct BitPath {
    // but we also store the path pointer inline rather than grabbing it from the interner
    // - firstly, this is for performance to avoid lookups and refcell etc
    // - secondly, it's much easier to debug when you can actually see the value of the path in the debugger
    //   rather than just an opaque index
    // One may ask why even bother interning? It's less for performance rather than just convenience of having
    // our path representation be a copy type
    path: &'static OsStr,
}

impl BitPath {
    pub(crate) const fn new(path: &'static OsStr) -> Self {
        Self { path }
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        Self::intern(OsStr::from_bytes(bytes))
    }

    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    pub fn with_extension(self, ext: impl AsRef<OsStr>) -> Self {
        Self::intern(self.as_path().with_extension(ext))
    }

    pub fn parent(self) -> Option<Self> {
        self.as_path().parent().map(Self::intern)
    }

    /// return the filename of a path, empty path if no filename
    pub fn file_name(self) -> Self {
        Self::intern(self.as_path().file_name().unwrap_or_else(|| OsStr::new("")))
    }

    #[inline]
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

    #[inline]
    pub fn intern_str(s: impl AsRef<str>) -> Self {
        // this must be outside the `interner` closure as the `as_ref` impl may use the interner
        // leading to refcell panics
        // quite questionable turning paths into strings and then bytes
        // probably not very platform agnostic
        let s = s.as_ref();
        with_path_interner(|interner| interner.intern_path(s))
    }

    #[inline]
    pub fn intern(path: impl AsRef<OsStr>) -> Self {
        with_path_interner(|interner| interner.intern_path(path))
    }

    #[inline]
    pub fn as_str(self) -> &'static str {
        self.as_path().to_str().unwrap()
    }

    #[inline]
    pub fn as_path(self) -> &'static Path {
        Path::new(self.path)
    }

    /// returns the components of a path
    /// foo/bar/baz -> [foo, bar, baz]
    pub fn components(self) -> impl Iterator<Item = BitPath> {
        self.as_path().iter().map(BitPath::new)
    }

    /// similar to `[BitPath::components](crate::path::BitPath::components)`
    /// foo/bar/baz -> [foo, foo/bar, foo/bar/baz]
    pub fn cumulative_components(self) -> impl Iterator<Item = BitPath> {
        let path = self.path.as_bytes();
        let mut index = 0;
        let mut done = false;
        std::iter::from_fn(move || {
            if done {
                return None;
            }
            index += 1 + match path[index..].iter().position(|&b| b == b'/') {
                Some(i) => i,
                None => {
                    done = true;
                    return Some(self);
                }
            };
            Some(Self::new(OsStr::from_bytes(&path[..index - 1])))
        })
    }

    #[inline]
    pub fn len(self) -> usize {
        self.as_os_str().len()
    }

    #[inline]
    pub fn as_os_str(self) -> &'static OsStr {
        self.path
    }

    #[inline]
    pub fn as_bytes(self) -> &'static [u8] {
        self.as_path().as_os_str().as_bytes()
    }

    /// returns first component of the path
    pub fn root_component(self) -> &'static Path {
        self.as_path().iter().next().unwrap().as_ref()
    }

    pub(crate) fn approximate_lexicographical_successor(self) -> BitPath {
        let mut s = self.path.to_str().unwrap().to_owned();
        s.push('a');
        Self::intern(s)
    }
}

impl Borrow<Path> for BitPath {
    fn borrow(&self) -> &Path {
        self.as_path()
    }
}

impl PartialEq for BitPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Hash for BitPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state)
    }
}

impl PartialEq<String> for BitPath {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl<'a> PartialEq<&'a OsStr> for BitPath {
    fn eq(&self, other: &&OsStr) -> bool {
        self.as_os_str() == *other
    }
}

impl PartialEq<str> for BitPath {
    fn eq(&self, other: &str) -> bool {
        self.as_os_str() == other
    }
}

impl<'a> PartialEq<&'a str> for BitPath {
    fn eq(&self, other: &&str) -> bool {
        self.as_os_str() == *other
    }
}

impl AsRef<OsStr> for BitPath {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
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

impl<'a> From<&'a str> for BitPath {
    fn from(s: &'a str) -> Self {
        Self::intern(s)
    }
}

impl Deref for BitPath {
    type Target = Path;

    fn deref(&self) -> &'static Self::Target {
        self.as_path()
    }
}

impl Debug for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_path().display())
    }
}

impl Deserialize for BitPath {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_to_str().map(Self::intern)?)
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
    //
    /// *IMPORTANT*: directories must have a trailing ascii character character > '/') for this ordering to be correct
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // doesn't make sense to compare relative with absolute and vice versa
        debug_assert_eq!(self.is_relative(), other.is_relative());
        Self::path_cmp(self, other)
    }
}

impl BitPath {
    pub fn path_cmp_explicit(
        a: impl AsRef<OsStr>,
        a_is_dir: bool,
        b: impl AsRef<OsStr>,
        b_is_dir: bool,
    ) -> std::cmp::Ordering {
        // files with the same subpath should come before directories
        let a = a.as_ref().as_bytes();
        let b = b.as_ref().as_bytes();
        let min_len = std::cmp::min(a.len(), b.len());

        a[..min_len].cmp(&b[..min_len]).then_with(|| {
            // Basically, if the path we are "at the end of" is a directory then
            // we give it a '/' otherwise a null byte (which is less than anything so files come first).
            // If it's not at the end then we give it the byte at that position and we compare.
            let x = if a.len() == min_len {
                if a_is_dir { b'/' } else { b'\0' }
            } else {
                a[min_len]
            };

            let y = if b.len() == min_len {
                if b_is_dir { b'/' } else { b'\0' }
            } else {
                b[min_len]
            };
            x.cmp(&y)
        })
    }

    /// Directories must have a trailing slash before calling this
    /// Otherwise use `path_cmp_explicit`
    pub fn path_cmp(a: impl AsRef<OsStr>, b: impl AsRef<OsStr>) -> std::cmp::Ordering {
        // files with the same subpath should come before directories
        let a = a.as_ref().as_bytes();
        let b = b.as_ref().as_bytes();
        let m = a.len();
        let n = b.len();
        let minlen = std::cmp::min(m, n);
        a[..minlen].cmp(&b[..minlen]).then_with(|| m.cmp(&n))
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
