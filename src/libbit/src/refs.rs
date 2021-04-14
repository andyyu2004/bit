use crate::error::{BitGenericError, BitResult};
use crate::hash::BitHash;
use crate::obj::BitObjKind;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::str::FromStr;

#[derive(Debug, Copy, Clone)]
pub enum BitRef {
    /// refers directly to an object
    Direct(BitHash),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `BitPath` contains `refs/remote/origin/master`
    /// possibly bitpath is not the best representation but its ok for now
    Indirect(BitPath),
}

impl Display for BitRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitRef::Direct(hash) => write!(f, "{}", hash),
            BitRef::Indirect(path) => write!(f, "{}", path),
        }
    }
}

impl Serialize for BitRef {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writer.write_all(self.to_string().as_bytes())?)
    }
}

impl Deserialize for BitRef {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut s = String::new();
        reader.read_to_string(&mut s)?;
        s.parse()
    }
}

impl FromStr for BitRef {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // probably fair to assume that a valid hash is not an indirect path
        if let Ok(hash) = BitHash::from_str(s) {
            return Ok(Self::Direct(hash));
        }
        // TODO validation of indirect?
        Ok(Self::Indirect(BitPath::intern(s)))
    }
}

impl BitRef {
    pub fn resolve(&self, repo: &BitRepo) -> BitResult<BitHash> {
        match self {
            BitRef::Direct(hash) => Ok(*hash),
            BitRef::Indirect(_path) => {
                // self.resolve_ref(r)
                todo!();
            }
        }
    }
}

impl BitRepo {
    pub fn resolve_ref(&self, r: BitRef) -> BitResult<BitHash> {
        r.resolve(self)
    }

    pub fn read_ref(&self, r: BitRef) -> BitResult<BitObjKind> {
        self.resolve_ref(r).and_then(|hash| self.read_obj(hash))
    }
}
