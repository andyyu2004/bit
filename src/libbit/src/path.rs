use crate::error::BitResult;
use crate::interner::{with_path_interner, with_path_interner_mut};
use crate::io_ext::ReadExt;
use std::borrow::Borrow;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs::File;
use std::ops::{Deref, Index};
use std::path::Path;
use std::slice::SliceIndex;

/// interned path (where path is just a string)
// interning paths is likely not worth it, but its nice to have it as a copy type
// since its used so much, this will also lend itself to faster comparisons as
// its now just an integer compare
#[derive(Eq, PartialEq, Clone, Copy, Hash)]
pub struct BitPath(u32);

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

    pub fn join(self, path: impl AsRef<Path>) -> Self {
        Self::intern(self.as_path().join(path))
    }

    pub fn read_to_vec(self) -> BitResult<Vec<u8>> {
        Ok(File::open(self)?.read_to_vec()?)
    }

    pub fn intern(path: impl AsRef<Path>) -> Self {
        // this must be outside the `interner` closure as the `as_ref` impl may use the interner
        // leading to refcell panics
        let path = path.as_ref();
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

use colored::*;
impl Colorize for BitPath {
    fn color<S: Into<Color>>(self, color: S) -> ColoredString {
        self.as_str().color(color)
    }

    fn on_color<S: Into<Color>>(self, color: S) -> ColoredString {
        self.as_str().on_color(color)
    }

    fn clear(self) -> ColoredString {
        self.as_str().clear()
    }

    fn normal(self) -> ColoredString {
        self.as_str().normal()
    }

    fn bold(self) -> ColoredString {
        self.as_str().bold()
    }

    fn dimmed(self) -> ColoredString {
        self.as_str().dimmed()
    }

    fn italic(self) -> ColoredString {
        self.as_str().italic()
    }

    fn underline(self) -> ColoredString {
        self.as_str().underline()
    }

    fn blink(self) -> ColoredString {
        self.as_str().blink()
    }

    fn reverse(self) -> ColoredString {
        self.as_str().reverse()
    }

    fn reversed(self) -> ColoredString {
        self.as_str().reversed()
    }

    fn hidden(self) -> ColoredString {
        self.as_str().hidden()
    }

    fn strikethrough(self) -> ColoredString {
        self.as_str().strikethrough()
    }
}

#[cfg(test)]
mod tests;
