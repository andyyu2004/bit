use super::Oid;
use crate::error::BitGenericError;
use crate::hash::BitPartialHash;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// ways an object can be identified
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum BitId {
    Full(Oid),
    Partial(BitPartialHash),
}

impl<'a> From<&'a str> for BitId {
    fn from(s: &'a str) -> Self {
        Self::from_str(s).unwrap()
    }
}

impl BitId {
    pub fn into_oid(self) -> Oid {
        if let Self::Full(v) = self { v } else { panic!("expected oid") }
    }
}

impl From<BitPartialHash> for BitId {
    fn from(v: BitPartialHash) -> Self {
        Self::Partial(v)
    }
}

impl From<Oid> for BitId {
    fn from(hash: Oid) -> Self {
        Self::Full(hash)
    }
}

impl Display for BitId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitId::Full(hash) => write!(f, "{}", hash),
            BitId::Partial(_) => todo!(),
        }
    }
}

impl FromStr for BitId {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("bit hashes should only contain ascii hex digits")
        }

        if s.len() == 40 {
            Ok(Self::Full(Oid::from_str(s).unwrap()))
        } else if s.len() < 40 {
            Ok(Self::Partial(BitPartialHash::from_str(s).unwrap()))
        } else {
            bail!("invalid bit object id: `{}`", s)
        }
    }
}
