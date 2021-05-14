use crate::error::BitGenericError;
use crate::hash::SHA1Hash;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

pub type Oid = SHA1Hash;

/// ways an object can be identified
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum BitId {
    Full(Oid),
    Partial(PartialOid),
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

impl From<PartialOid> for BitId {
    fn from(v: PartialOid) -> Self {
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
            Ok(Self::Partial(PartialOid::from_str(s).unwrap()))
        } else {
            bail!("invalid bit object id: `{}`", s)
        }
    }
}

// basically the same type as Oid just with different (fewer) invariants
// this is 40 bytes long instead of 20 like `Oid`
// as otherwise its a bit difficult to handle odd length input strings
// because we'd have to deal with half bytes
#[derive(PartialEq, Eq, Debug, Hash, Clone, Ord, PartialOrd, Copy)]
pub struct PartialOid([u8; 40]);

impl FromStr for PartialOid {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ensure!(s.len() < 40, "creating partial hash with an invalid hex string (too long)");
        ensure!(s.len() >= 4, "bit hash prefix must be at least 4 hex characters");
        let mut buf = [0u8; 40];
        buf.as_mut().write_all(s.as_bytes())?;
        Ok(Self(buf))
    }
}

impl<I> Index<I> for PartialOid
where
    I: SliceIndex<[u8]>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}
