use lazy_static::lazy_static;
use regex::Regex;

use crate::error::{BitGenericError, BitResult};
use crate::lockfile::Lockfile;
use crate::obj::{BitId, Oid};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::str::FromStr;

lazy_static! {
    /// defines what is an invalid reference name (anything else is valid)
    // a reference name is invalid if any of the following conditions are true
    // - any path component begins with `.`
    // - contains `..`
    // - contains any of the following `*` `:` `?` `[` `\` `^` `~` <space> <tab>
    // - ends with `/` or `.lock`
    // - contains `@{`

    static ref INVALID_REF_REGEX: Regex = Regex::new(r#"^\.|\.\.|\*|:|\?|\[|\\|^|~| |\t|/$|\.lock$|@\{"#).unwrap();
}

pub fn is_valid_name(s: &str) -> bool {
    !INVALID_REF_REGEX.is_match(s)
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum BitRef {
    /// refers directly to an object
    Direct(BitId),
    /// contains the path of another reference
    /// if the ref is `ref: refs/remote/origin/master`
    /// then the `BitPath` contains `refs/remote/origin/master`
    /// possibly bitpath is not the best representation but its ok for now
    Symbolic(SymbolicRef),
}

impl BitRepo {
    /// returns `None` if the reference does not yet exist
    // don't think this can be written in terms of `fully_resolve_ref` below
    // if we were to do something like `fully_resolve_ref().ok()`, then all errors will result in None
    // which is not quite right
    pub fn try_fully_resolve_ref(&self, r: impl Into<BitRef>) -> BitResult<Option<Oid>> {
        match r.into().resolve(self)? {
            ResolvedRef::Resolved(oid) => Ok(Some(oid)),
            ResolvedRef::NonExistent(_) => Ok(None),
        }
    }

    pub fn fully_resolve_ref(&self, r: impl Into<BitRef>) -> BitResult<Oid> {
        let r = r.into();
        match r.resolve(self)? {
            ResolvedRef::Resolved(oid) => Ok(oid),
            ResolvedRef::NonExistent(sym) =>
                bail!("failed to fully resolve ref `{}`: references nonexistent file `{}`", r, sym),
        }
    }

    pub fn resolve_ref(&self, r: BitRef) -> BitResult<ResolvedRef> {
        r.resolve(self)
    }
}

impl From<Oid> for BitRef {
    fn from(oid: Oid) -> Self {
        Self::Direct(BitId::from(oid))
    }
}

impl From<SymbolicRef> for BitRef {
    fn from(sym: SymbolicRef) -> Self {
        Self::Symbolic(sym)
    }
}

impl From<BitId> for BitRef {
    fn from(id: BitId) -> Self {
        Self::Direct(id)
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
        // probably fair to assume that a valid bitid(i.e. partial/full hash) is not an indirect path?
        if let Ok(id) = BitId::from_str(s) {
            return Ok(Self::Direct(id));
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

lazy_static! {
    static ref SYM_REF_REGEX: Regex = Regex::new(r#"\.(b|g)it/(HEAD$|refs/heads/)"#).unwrap();
}

impl SymbolicRef {
    pub fn new(path: BitPath) -> Self {
        debug_assert!(SYM_REF_REGEX.is_match(path.as_str()), "invalid symref `{}`", path);
        Self { path }
    }
}

impl Display for SymbolicRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
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

// slightly more constrained version of `BitRef` (no partial oids)
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ResolvedRef {
    /// fully resolved commit oid
    Resolved(Oid),
    /// the branch pointed to by the symref doesn't exist yet
    NonExistent(SymbolicRef),
}

impl ResolvedRef {
    pub fn try_into_oid(self) -> BitResult<Oid> {
        match self {
            ResolvedRef::Resolved(oid) => Ok(oid),
            ResolvedRef::NonExistent(sym) =>
                bail!("branch `{}` does not exist. Try creating a commit on the branch first", sym),
        }
    }
}

impl BitRef {
    /// resolves the reference one layer
    pub fn partially_resolve(&self, repo: &BitRepo) -> BitResult<ResolvedRef> {
        match self {
            BitRef::Direct(id) => match *id {
                BitId::Full(oid) => Ok(ResolvedRef::Resolved(oid)),
                BitId::Partial(partial) => repo.expand_prefix(partial).map(ResolvedRef::Resolved),
            },
            BitRef::Symbolic(sym) => Ok(ResolvedRef::NonExistent(*sym)),
        }
    }

    /// resolves the reference as much as possible
    /// if the symref points to a path that doesn't exist, then the value of the symref itself is returned
    /// i.e. if `HEAD` -> `refs/heads/master` which doesn't yet exist, then `refs/heads/master` will be returned
    /// returns iff a symbolic ref points at a non-existing branch
    pub fn resolve(&self, repo: &BitRepo) -> BitResult<ResolvedRef> {
        match self.partially_resolve(repo)? {
            ResolvedRef::Resolved(oid) => Ok(ResolvedRef::Resolved(oid)),
            ResolvedRef::NonExistent(sym) => {
                let ref_path = repo.relative_path(sym.path);
                if !ref_path.exists() {
                    return Ok(ResolvedRef::NonExistent(sym));
                }

                let resolved_ref = Lockfile::with_readonly(ref_path, |_| {
                    let contents = std::fs::read_to_string(ref_path)?;
                    // symbolic references can be recursive
                    // i.e. HEAD -> refs/heads/master -> <oid>
                    BitRef::from_str(contents.trim_end())?.resolve(repo)
                })?;

                if let ResolvedRef::Resolved(oid) = resolved_ref {
                    ensure!(
                        repo.obj_exists(oid)?,
                        "invalid reference: reference at `{}` which contains invalid object hash `{}` (from symbolic reference `{}`)",
                        ref_path,
                        oid,
                        sym
                    );
                }

                debug!("BitRef::resolve: resolved ref `{:?}` to `{:?}`", sym, resolved_ref);

                Ok(resolved_ref)
            }
        }
    }
}

pub struct BitRefDb {
    bitdir: BitPath,
}

impl BitRefDb {
    pub fn new(bitdir: BitPath) -> Self {
        Self { bitdir }
    }
}

pub trait BitRefDbBackend {
    fn create(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    fn resolve(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    // may implicitly create the ref
    fn update(&self, sym: SymbolicRef, to: BitRef) -> BitResult<()>;
    fn delete(&self, sym: SymbolicRef) -> BitResult<()>;
    fn exists(&self, sym: SymbolicRef) -> BitResult<bool>;
}

impl BitRefDbBackend for BitRefDb {
    fn create(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        todo!()
    }

    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        Lockfile::with_readonly(self.bitdir.join(sym.path), |lockfile| {
            let head_file =
                lockfile.file().unwrap_or_else(|| panic!("ref `{}` does not exist", sym));
            BitRef::deserialize_unbuffered(head_file)
        })
    }

    fn resolve(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        todo!()
    }

    fn update(&self, sym: SymbolicRef, to: BitRef) -> BitResult<()> {
        Lockfile::with_mut(self.bitdir.join(sym.path), |lockfile| to.serialize(lockfile))
    }

    fn delete(&self, sym: SymbolicRef) -> BitResult<()> {
        todo!()
    }

    fn exists(&self, sym: SymbolicRef) -> BitResult<bool> {
        Ok(self.bitdir.join(sym.path).exists())
    }
}

#[cfg(test)]
mod tests;
