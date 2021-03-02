use crate::error::BitResult;
use crate::obj::{self, BitObj};
use sha1::{Digest, Sha1};
use std::convert::TryInto;
use std::fmt::{self, Display, Formatter};
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct SHA1Hash([u8; 20]);

impl SHA1Hash {
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// split hash into the first two hex digits and the rest
    /// for use in finding <directory>/<file>
    pub fn split(&self) -> (String, String) {
        (hex::encode(&self[0..1]), hex::encode(&self[1..]))
    }
}

impl FromStr for SHA1Hash {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // only call this when preconditions are met
        assert!(s.len() == 40, "SHA1 called with invalid hex string");
        Ok(Self(hex::decode(s).unwrap().try_into().unwrap()))
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

impl Display for SHA1Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self))
    }
}

pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> SHA1Hash {
    // use sha1 to be more compatible with current git
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    SHA1Hash(hasher.finalize().into())
}

pub fn hash_obj(obj: &impl BitObj) -> BitResult<SHA1Hash> {
    let bytes = obj::serialize_obj_with_headers(obj)?;
    Ok(hash_bytes(bytes.as_slice()))
}
