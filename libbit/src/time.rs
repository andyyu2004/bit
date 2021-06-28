use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::fs::Metadata;
use std::os::unix::prelude::MetadataExt;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(BitArbitrary))]
pub struct Timespec {
    pub sec: u32,
    pub nano: u32,
}

impl Timespec {
    pub fn zero() -> Self {
        Self::new(0, 0)
    }

    pub fn new(sec: u32, nano: u32) -> Self {
        Self { sec, nano }
    }

    pub fn new_i64(sec: i64, nano: i64) -> Self {
        debug_assert!(sec < u32::MAX as i64);
        debug_assert!(nano < u32::MAX as i64);
        Self::new(sec as u32, nano as u32)
    }

    pub fn ctime(metadata: &Metadata) -> Self {
        Self::new_i64(metadata.ctime(), metadata.ctime_nsec())
    }

    pub fn mtime(metadata: &Metadata) -> Self {
        Self::new_i64(metadata.mtime(), metadata.mtime_nsec())
    }
}

impl Display for Timespec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.sec, self.nano)
    }
}

impl PartialOrd for Timespec {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timespec {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sec.cmp(&other.sec).then_with(|| self.nano.cmp(&other.nano))
    }
}
