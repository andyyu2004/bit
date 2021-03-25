use crate::error::{BitError, BitResult};
use crate::obj::{self, BitObj};
use crate::path::BitPath;
use sha1::digest::Output;
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

pub type BitHash = SHA1Hash;
pub const BIT_HASH_SIZE: usize = std::mem::size_of::<BitHash>();

#[derive(PartialEq, Eq, Hash, Clone, Ord, PartialOrd, Copy)]
#[repr(transparent)]
pub struct SHA1Hash([u8; 20]);

impl From<Output<Sha1>> for SHA1Hash {
    fn from(bytes: Output<Sha1>) -> Self {
        Self::new(bytes.try_into().unwrap())
    }
}

impl BitHash {
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// split hash into the first two hex digits (hence first byte)
    /// and the rest for use in finding <directory>/<file>
    pub fn split(&self) -> (BitPath, BitPath) {
        (BitPath::intern(hex::encode(&self[0..1])), BitPath::intern(hex::encode(&self[1..])))
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for BitHash {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self((0..20).map(|_| u8::arbitrary(g)).collect::<Vec<_>>().try_into().unwrap())
    }
}

impl FromStr for BitHash {
    type Err = BitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // only call this when preconditions are met
        assert!(s.len() == 40, "SHA1 called with invalid hex string");
        Ok(Self(hex::decode(s).unwrap().try_into().unwrap()))
    }
}

impl AsRef<[u8]> for BitHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<I> Index<I> for BitHash
where
    I: SliceIndex<[u8]>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}

impl Debug for BitHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for BitHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self))
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> BitHash {
    // use sha1 to be more compatible with current git
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    BitHash::new(hasher.finalize().into())
}

pub fn hash_obj(obj: &impl BitObj) -> BitResult<BitHash> {
    let bytes = obj::serialize_obj_with_headers(obj)?;
    Ok(hash_bytes(bytes.as_slice()))
}
