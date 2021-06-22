use crate::error::{BitGenericError, BitResult};
use crate::obj::{Oid, WritableObject};
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
    /// hash of an empty file
    // e69de29bb2d1d6434b8b29ae775ad8c2e48c5391
    pub const EMPTY_BLOB: Self = Self([
        0xe6, 0x9d, 0xe2, 0x9b, 0xb2, 0xd1, 0xd6, 0x43, 0x4b, 0x8b, 0x29, 0xae, 0x77, 0x5a, 0xd8,
        0xc2, 0xe4, 0x8c, 0x53, 0x91,
    ]);
    /// hash of an empty tree
    // 4b825dc642cb6eb9a060e54bf8d69288fbee4904
    pub const EMPTY_TREE: Self = Self([
        0x4b, 0x82, 0x5d, 0xc6, 0x42, 0xcb, 0x6e, 0xb9, 0xa0, 0x60, 0xe5, 0x4b, 0xf8, 0xd6, 0x92,
        0x88, 0xfb, 0xee, 0x49, 0x04,
    ]);
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

pub fn hash_obj<O: WritableObject + ?Sized>(obj: &O) -> BitResult<SHA1Hash> {
    let bytes = obj.serialize_with_headers()?;
    Ok(hash_bytes(bytes.as_slice()))
}
