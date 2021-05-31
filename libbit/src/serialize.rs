use crate::error::BitResult;
use std::io::{prelude::*, BufReader};
use std::str::FromStr;

pub trait Serialize {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()>;
}

pub trait BufReadSeek: BufRead + Seek {}

impl<R: BufRead + Seek> BufReadSeek for R {
}

// we use some explicit `Self: Sized` bounds on each function
// (instead of putting the bound on the trait) for object safety
// we ignore these methods as they are obviously not object safe
// however, we do wish BitObj to be object safe
// this is essentially an empty trait when used as a trait object
pub trait Deserialize {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized;

    fn deserialize_unbuffered(reader: impl Read) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize(&mut BufReader::new(reader))
    }
}

/// deserialize trait where the size to read is required to be known
/// confusingly, the size given is not necessarily the exact number of bytes that will be
/// read from the reader.
/// therefore, we unfortunately cannot combine this trait with `Deserialize` where
/// `deserialize_unbuffered(reader, size) is defined as `deserialize(reader.take(size))`
/// as it is not general enough for all our purposes
/// For example, in [crate::obj::RefDelta] and [crate::obj::OfsDelta], the size parameter is interpreted
/// as the size of the delta not not including the offset/baseoid.
pub trait DeserializeSized {
    fn deserialize_sized(reader: &mut impl BufRead, size: u64) -> BitResult<Self>
    where
        Self: Sized;

    fn deserialize_from_slice(slice: &[u8]) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize_sized_unbuffered(slice, slice.len() as u64)
    }

    fn deserialize_sized_unbuffered(reader: impl Read, size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize_sized(&mut BufReader::new(reader), size)
    }
}

impl<D: Deserialize> DeserializeSized for D {
    fn deserialize_sized(reader: &mut impl BufRead, _size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize(reader)
    }
}
