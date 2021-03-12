use crate::interner::with_interner;
use std::fmt::{self, Display, Formatter};

// interning paths is likely not worth it, but its nice to have it as a copy type
// since its used so much, this will also lend itself to faster comparisons as
// its now just an integer compare
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct BitPath(u32);

impl BitPath {
    pub fn new(u: u32) -> Self {
        Self(u)
    }

    pub fn index(self) -> u32 {
        self.0
    }

    pub fn intern(s: &str) -> Self {
        with_interner(|interner| interner.intern(s))
    }

    pub fn path(self) -> &'static str {
        with_interner(|interner| interner.get_str(self))
    }
}

impl Display for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path())
    }
}

impl BitPath {
    pub fn len(&self) -> usize {
        self.path().len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.path().as_bytes()
    }
}

impl PartialOrd for BitPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}
