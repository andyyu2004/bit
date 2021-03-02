use crate::{hash::SHA1Hash, BitError};
use std::str::FromStr;

/// ways an object can be identified
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum BitObjId {
    FullHash(SHA1Hash),
    PartialHash(String),
}

impl FromStr for BitObjId {
    type Err = BitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 40 {
            Ok(Self::FullHash(SHA1Hash::from_str(s).unwrap()))
        } else if s.len() == 7 {
            Ok(Self::PartialHash(s.to_owned()))
        } else {
            panic!("invalid id")
        }
    }
}
