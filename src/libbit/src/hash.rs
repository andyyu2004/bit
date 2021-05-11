use crate::error::{BitGenericError, BitResult};
use crate::obj::BitObj;
use crate::path::BitPath;
use sha1::digest::Output;
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::prelude::*;
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

pub const BIT_HASH_SIZE: usize = std::mem::size_of::<BitHash>();

pub type BitHash = SHA1Hash;

#[derive(PartialEq, Eq, Hash, Clone, Ord, PartialOrd, Copy)]
#[repr(transparent)]
pub struct SHA1Hash([u8; 20]);

impl From<Output<Sha1>> for SHA1Hash {
    fn from(bytes: Output<Sha1>) -> Self {
        Self::new(bytes.try_into().unwrap())
    }
}

impl BitHash {
    /// this represents an unknown hash
    // didn't find anywhere that SHA1 can't return 0
    // but libgit2 also uses this special value
    // and its so incredibly unlikely even if it is possible
    pub const ZERO: Self = Self([0; 20]);

    #[inline]
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    #[inline]
    pub fn is_unknown(self) -> bool {
        self == BitHash::ZERO
    }

    #[inline]
    pub fn is_known(self) -> bool {
        self != BitHash::ZERO
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

// basically the same type as BitHash just with different (fewer) invariants
// this is 40 bytes long instead of 20 like `BitHash`
// as otherwise its a bit difficult to handle odd length input strings
#[derive(PartialEq, Eq, Debug, Hash, Clone, Ord, PartialOrd, Copy)]
pub struct BitPartialHash([u8; 40]);

impl FromStr for BitPartialHash {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ensure!(s.len() < 40, "creating partial hash with an invalid hex string (too long)");
        let mut buf = [0u8; 40];
        buf.as_mut().write_all(s.as_bytes())?;
        Ok(Self(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_partial_hash() -> BitResult<()> {
        let hash = BitPartialHash::from_str("8e3")?;
        assert_eq!(&hash.0[0..3], b"8e3");
        assert_eq!(hash.0[3..], [0u8; 37]);
        Ok(())
    }
}

impl FromStr for BitHash {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ensure!(s.len() == 40, "creating SHA1 with invalid hex string (incorrect length)");
        let mut buf = [0u8; 20];
        hex::decode_to_slice(s, &mut buf)?;
        Ok(Self(buf))
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

pub fn crc_of(bytes: impl AsRef<[u8]>) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes.as_ref());
    crc.sum()
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> BitHash {
    // use sha1 to be more compatible with current git
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    BitHash::new(hasher.finalize().into())
}

pub fn hash_obj(obj: &impl BitObj) -> BitResult<BitHash> {
    let bytes = obj.serialize_with_headers()?;
    Ok(hash_bytes(bytes.as_slice()))
}
