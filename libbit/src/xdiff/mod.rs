use std::ops::Index;

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

impl<T> Index<isize> for OffsetVec<T> {
    type Output = T;

    fn index(&self, index: isize) -> &Self::Output {
        &self.base[(index - self.offset) as usize]
    }
}

struct MyersDiff<'a, I, J> {
    something: &'a (),
    a: I,
    b: J,
    v: OffsetVec<u32>,
}

trait LinesIter<'a> = ExactSizeIterator<Item = &'a str>;

impl<'a, I: LinesIter<'a>, J: LinesIter<'a>> MyersDiff<'a, I, J> {
    pub fn new(a: I, b: J) -> Self {
        let m = a.len();
        let n = b.len();
        let size = m + n;
        let v = OffsetVec::with_capacity(size as isize, size * 2);
        Self { a, b, v, something: &() }
    }

    pub fn diff(
        a: impl ExactSizeIterator<Item = &'a str>,
        b: impl ExactSizeIterator<Item = &'a str>,
    ) {
    }
}
