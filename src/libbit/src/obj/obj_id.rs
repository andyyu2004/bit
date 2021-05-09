use crate::error::BitGenericError;
use crate::hash::{BitHash, BitPartialHash};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// ways an object can be identified
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum BitId {
    Full(BitHash),
    Partial(BitPartialHash),
}

impl From<BitPartialHash> for BitId {
    fn from(v: BitPartialHash) -> Self {
        Self::Partial(v)
    }
}

impl From<BitHash> for BitId {
    fn from(hash: BitHash) -> Self {
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
            Ok(Self::Full(BitHash::from_str(s).unwrap()))
        } else if s.len() < 40 {
            Ok(Self::Partial(BitPartialHash::from_str(s).unwrap()))
        } else {
            bail!("invalid bit object id: `{}`", s)
        }
    }
}
