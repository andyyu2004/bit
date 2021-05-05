use crate::error::BitResult;
use std::io::{prelude::*, BufReader};

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
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized;

    fn deserialize_unbuffered(reader: impl Read) -> BitResult<Self>
    where
        Self: Sized,
    {
        Self::deserialize(&mut BufReader::new(reader))
    }
}
