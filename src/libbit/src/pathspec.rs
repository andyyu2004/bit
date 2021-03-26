use crate::error::BitGenericError;
use crate::path::BitPath;
use itertools::Itertools;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct Pathspec {
    /// non-wildcard prefix
    /// up to either the first wildcard or the last slash
    prefix: BitPath,
    pathspec: Vec<()>,
}

impl Pathspec {
    pub fn parse_prefix(s: &str) -> BitPath {
        todo!()
    }
}

pub struct FnMatch {
    path: BitPath,
    parent: BitPath,
}

impl FromStr for Pathspec {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split("/").collect_vec();
        todo!()
    }
}
