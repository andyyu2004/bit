mod refdb;
mod reflog;

use crate::error::{BitGenericError, BitResult};

use crate::obj::{BitObjKind, Oid, Tree, Treeish};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::str::FromStr;

pub use refdb::*;
pub use reflog::*;

lazy_static! {
    /// defines what is an invalid reference name (anything else is valid)
    // a reference name is invalid if any of the following conditions are true
    // - any path component begins with `.` (i.e. `^.`, or `/.`)
    // - contains `..`
    // - contains any of the following `*` `:` `?` `[` `\` `^` `~` <space> <tab>
    // - ends with `/` or `.lock`
    // - contains `@{`
    static ref INVALID_REF_REGEX: Regex = Regex::new(r#"^\.|/\.|\.\.|\*|:|\?|\[|\\|\^|~| |\t|/$|\.lock$|@\{"#).unwrap();
}

pub fn is_valid_name(s: &str) -> bool {
    !INVALID_REF_REGEX.is_match(s)
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum BitRef {
    /// refers directly to an object
    Direct(Oid),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `BitPath` contains `refs/remote/origin/master`
    /// possibly bitpath is not the best representation but its ok for now
    Symbolic(SymbolicRef),
}

impl From<Oid> for BitRef {
    fn from(oid: Oid) -> Self {
        Self::Direct(oid)
    }
}

impl From<SymbolicRef> for BitRef {
    fn from(sym: SymbolicRef) -> Self {
        Self::Symbolic(sym)
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
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
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
        // probably fair to assume that a valid oid is not an indirect path
        if let Ok(oid) = Oid::from_str(s) {
            return Ok(Self::Direct(oid));
        }
        // TODO validation of indirect?
        SymbolicRef::from_str(s).map(Self::Symbolic)
    }
}

// symbolic ref is of the form `ref: <ref>`
const SYMBOLIC_REF_PREFIX: &str = "ref: ";

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct SymbolicRef {
    path: BitPath,
}

impl SymbolicRef {
    pub const HEAD: Self = Self { path: BitPath::HEAD };

    pub fn new(path: BitPath) -> Self {
        debug_assert!(path.is_relative());
        Self { path }
    }

    pub fn branch(name: &str) -> Self {
        Self::new(BitPath::intern(format!("refs/heads/{}", name)))
    }
}

impl Display for SymbolicRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // alternate is used to display to the user (cutting off prefix `refs/heads`)
        let s = self.path.as_str();
        if f.alternate() {
            write!(f, "{}", s.strip_prefix("refs/heads/").unwrap_or(s))
        } else {
            write!(f, "{}", s)
        }
    }
}

impl FromStr for SymbolicRef {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let r = if s.starts_with(SYMBOLIC_REF_PREFIX) {
            s.split_at(SYMBOLIC_REF_PREFIX.len()).1
        } else {
            // support parsing symbolic_ref without the prefix for use in revs
            // maybe a better way
            s
        };
        // TODO validation on r
        Ok(Self { path: BitPath::intern(r.trim_end()) })
    }
}

impl BitRef {
    pub const HEAD: Self = Self::Symbolic(SymbolicRef::HEAD);

    pub fn resolve_to_tree(self, repo: BitRepo<'_>) -> BitResult<Tree> {
        let oid = repo.fully_resolve_ref(self)?;
        match repo.read_obj(oid)? {
            BitObjKind::Blob(..) => bail!("blob type is not treeish"),
            BitObjKind::Commit(commit) => repo.read_obj(commit.tree)?.into_tree(),
            BitObjKind::Tree(tree) => Ok(tree),
            BitObjKind::Tag(..) => todo!(),
        }
    }
}

#[cfg(test)]
mod tests;
