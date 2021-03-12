use std::fmt::{self, Display, Formatter};
// using string for now as paths are so painful
// maybe will run into os related issues
// but should be fine for now
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BitPath(String);

impl Display for BitPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for BitPath {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl<'a> From<&'a str> for BitPath {
    fn from(s: &'a str) -> Self {
        Self::from(s.to_owned())
    }
}

impl BitPath {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl PartialOrd for BitPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}
