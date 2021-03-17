use crate::error::BitError;
use crate::hash::BitHash;
use std::str::FromStr;

/// ways an object can be identified
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum BitObjId {
    FullHash(BitHash),
    PartialHash(String),
}

impl From<BitHash> for BitObjId {
    fn from(hash: BitHash) -> Self {
        Self::FullHash(hash)
    }
}

impl FromStr for BitObjId {
    type Err = BitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 40 {
            Ok(Self::FullHash(BitHash::from_str(s).unwrap()))
        } else if s.len() == 7 {
            Ok(Self::PartialHash(s.to_owned()))
        } else {
            panic!("invalid bit object id: `{}`", s)
        }
    }
}
