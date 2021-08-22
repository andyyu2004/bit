mod format;

pub use format::*;

use diffy::PatchFormatter;
use std::io::{self, Write};
use std::ops::{Index, IndexMut};

use crate::merge::ConflictStyle;

pub type BitPatch<'a> = diffy::Patch<'a, str>;

// TODO too lazy to implement all this rn so just using a library and see how it goes
pub fn xdiff<'a>(original: &'a str, modified: &'a str) -> BitPatch<'a> {
    diffy::create_patch(original, modified)
}

pub fn format_patch_into<W: Write>(writer: W, patch: &BitPatch<'_>) -> io::Result<()> {
    PatchFormatter::new().write_patch_into(patch, writer)
}

pub fn merge(
    conflict_style: ConflictStyle,
    ours_marker: impl AsRef<str>,
    theirs_marker: impl AsRef<str>,
    base: &[u8],
    a: &[u8],
    b: &[u8],
) -> Result<Vec<u8>, Vec<u8>> {
    diffy::MergeOptions::new()
        .set_conflict_style(conflict_style)
        .set_ours_marker(ours_marker.as_ref().to_owned())
        .set_theirs_marker(theirs_marker.as_ref().to_owned())
        .merge_bytes(base, a, b)
}

struct OffsetVec<T> {
    base: Vec<T>,
    offset: isize,
}

impl<T> OffsetVec<T> {
    pub fn new(offset: isize) -> Self {
        Self { offset, base: Default::default() }
    }

    pub fn with_capacity(offset: isize, capacity: usize) -> Self {
        Self { offset, base: Vec::with_capacity(capacity) }
    }
}

impl<T: Default + Clone> OffsetVec<T> {
    pub fn filled_with_capacity(offset: isize, capacity: usize) -> Self {
        Self { offset, base: vec![T::default(); capacity] }
    }
}

impl<T> Index<isize> for OffsetVec<T> {
    type Output = T;

    fn index(&self, index: isize) -> &Self::Output {
        &self.base[(index + self.offset) as usize]
    }
}

impl<T> IndexMut<isize> for OffsetVec<T> {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        &mut self.base[(index + self.offset) as usize]
    }
}

struct MyersDiff<'a, 's> {
    something: &'a (),
    a: &'a [&'s str],
    b: &'a [&'s str],
    v: OffsetVec<isize>,
}

pub fn xdiff_dist(a: &[&str], b: &[&str]) -> isize {
    MyersDiff::new(a, b).diff()
}

impl<'a, 's> MyersDiff<'a, 's> {
    pub fn new(a: &'a [&'s str], b: &'a [&'s str]) -> Self {
        let m = a.len();
        let n = b.len();
        let size = m + n;
        let v = OffsetVec::filled_with_capacity(size as isize, size * 2);
        Self { a, b, v, something: &() }
    }

    pub fn diff(mut self) -> isize {
        let v = &mut self.v;
        let a = self.a;
        let b = self.b;
        let m = a.len() as isize;
        let n = b.len() as isize;
        for d in 0..m + n {
            for k in (-d..=d).step_by(2) {
                let mut x = if k == -d || (k != d && v[k - 1] < v[k + 1]) {
                    v[k + 1]
                } else {
                    v[k - 1] + 1
                };
                let mut y = x - k;
                while x < m && y < n && a[x as usize] == b[y as usize] {
                    x += 1;
                    y += 1;
                }
                v[k] = x;
                if x == m && y == n {
                    return d;
                }
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod xdiff_format_tests;
