use crate::error::{BitGenericError, BitResult};
use crate::hash::BitHash;
use crate::lockfile::Lockfile;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::str::FromStr;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum BitRef {
    /// refers directly to an object
    Direct(BitHash),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `BitPath` contains `refs/remote/origin/master`
    /// possibly bitpath is not the best representation but its ok for now
    Symbolic(SymbolicRef),
}

impl From<BitHash> for BitRef {
    fn from(hash: BitHash) -> Self {
        Self::Direct(hash)
    }
}

impl Display for BitRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitRef::Direct(hash) => write!(f, "{}", hash),
            BitRef::Symbolic(path) => write!(f, "{}", path),
        }
    }
}

impl Serialize for BitRef {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writer.write_all(self.to_string().as_bytes())?)
    }
}

impl Deserialize for BitRef {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
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
        SymbolicRef::from_str(s).map(Self::Symbolic)
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct SymbolicRef {
    path: BitPath,
}

impl Display for SymbolicRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl FromStr for SymbolicRef {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // symbolic ref is of the form `ref: <ref>`
        const PREFIX: &str = "ref: ";
        let r = if s.starts_with(PREFIX) { s.split_at(PREFIX.len()).1 } else { s };
        // TODO validation on r
        Ok(Self { path: BitPath::intern(r.trim_end()) })
    }
}

impl BitRef {
    pub fn resolve(&self, repo: &BitRepo) -> BitResult<Option<BitHash>> {
        match self {
            BitRef::Direct(hash) => Ok(Some(*hash)),
            BitRef::Symbolic(sym) => {
                // TODO do we have to create the ref file if it doesn't exist yet?
                // i.e. HEAD points to refs/heads/master on initialization even when master doesn't exist
                // as there are no commits yet
                let ref_path = repo.relative_path(sym.path);
                if !ref_path.exists() {
                    return Ok(None);
                }
                let hash = Lockfile::with_readonly(ref_path, |_| {
                    let contents = std::fs::read_to_string(ref_path)?;
                    BitHash::from_str(contents.trim_end())
                })?;
                ensure!(
                    repo.obj_exists(hash)?,
                    "invalid reference: reference at `{}` which contains invalid object hash `{}` (from symbolic reference `{}`)",
                    ref_path,
                    hash,
                    sym
                );

                debug!("BitRef::resolve: resolved ref `{:?}` to commit `{}`", sym, hash);
                Ok(Some(hash))
            }
        }
    }
}

impl BitRepo {
    pub fn resolve_ref(&self, r: BitRef) -> BitResult<Option<BitHash>> {
        r.resolve(self)
    }
}
