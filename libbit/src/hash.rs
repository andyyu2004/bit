use crate::error::BitGenericError;
use crate::obj::Oid;
use rustc_hash::FxHasher;
use rustc_hex::{FromHex, ToHex};
use sha1::digest::Output;
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::Hasher;
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

pub const OID_SIZE: usize = std::mem::size_of::<Oid>();

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

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn short(&self) -> String {
        self.to_hex()[0..7].to_owned()
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
        let s = s.trim_end();
        ensure!(s.len() == 40, "creating SHA1 with invalid hex string (incorrect length)");
        let bytes = s.from_hex::<arrayvec::ArrayVec<u8, 20>>()?;
        // SAFETY, we just checked the length
        Ok(Self(unsafe { bytes.into_inner_unchecked() }))
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
        let hex = self.0.to_hex::<String>();
        if f.alternate() { write!(f, "{}", &hex[..7]) } else { write!(f, "{}", hex) }
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

pub trait MakeHash {
    fn mk_fx_hash(&self) -> u64;
}

impl<H: std::hash::Hash + ?Sized> MakeHash for H {
    #[inline]
    fn mk_fx_hash(&self) -> u64 {
        let mut state = FxHasher::default();
        self.hash(&mut state);
        state.finish()
    }
}
