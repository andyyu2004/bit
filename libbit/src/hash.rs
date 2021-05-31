use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObj, Oid};
use crate::path::BitPath;
use sha1::digest::Output;
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

pub const BIT_HASH_SIZE: usize = std::mem::size_of::<Oid>();

#[derive(PartialEq, Eq, Hash, Clone, Ord, PartialOrd, Copy)]
#[repr(transparent)]
pub struct SHA1Hash([u8; 20]);

impl From<Output<Sha1>> for SHA1Hash {
    fn from(bytes: Output<Sha1>) -> Self {
        Self::new(bytes.try_into().unwrap())
    }
}

// purely for convenience
#[cfg(test)]
impl<'a> From<&'a str> for SHA1Hash {
    fn from(s: &'a str) -> Self {
        Self::from_str(s).unwrap()
    }
}

impl SHA1Hash {
    /// this represents an unknown hash
    // didn't find anywhere that SHA1 can't return 0
    // but libgit2 also uses this special value
    // and its so incredibly unlikely even if it is possible
    pub const UNKNOWN: Self = Self([0; 20]);

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
        self == Self::UNKNOWN
    }

    #[inline]
    pub fn is_known(self) -> bool {
        self != Self::UNKNOWN
    }

    /// split hash into the first two hex digits (hence first byte)
    /// and the rest for use in finding <directory>/<file>
    pub fn split(&self) -> (BitPath, BitPath) {
        (BitPath::intern(hex::encode(&self[0..1])), BitPath::intern(hex::encode(&self[1..])))
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for SHA1Hash {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self((0..20).map(|_| u8::arbitrary(g)).collect::<Vec<_>>().try_into().unwrap())
    }
}

impl FromStr for SHA1Hash {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ensure!(s.len() == 40, "creating SHA1 with invalid hex string (incorrect length)");
        ensure!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "bit hashes should only contain ascii hex digits"
        );
        let mut buf = [0u8; 20];
        hex::decode_to_slice(s, &mut buf)?;
        Ok(Self(buf))
    }
}

impl AsRef<[u8]> for SHA1Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<I> Index<I> for SHA1Hash
where
    I: SliceIndex<[u8]>,
{
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}

impl Debug for SHA1Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for SHA1Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self))
    }
}

pub fn crc_of(bytes: impl AsRef<[u8]>) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes.as_ref());
    crc.sum()
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> SHA1Hash {
    // use sha1 to be more compatible with current git
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    SHA1Hash::new(hasher.finalize().into())
}

pub fn hash_obj(obj: &impl BitObj) -> BitResult<SHA1Hash> {
    let bytes = obj.serialize_with_headers()?;
    Ok(hash_bytes(bytes.as_slice()))
}
