use super::{BitRef, BitReflog, SymbolicRef};
use crate::error::{BitError, BitResult};
use crate::lockfile::{Filelock, Lockfile, LockfileFlags};
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::signature::BitSignature;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

pub struct BitRefDb<'rcx> {
    repo: BitRepo<'rcx>,
    bitdir: BitPath,
}

impl<'rcx> BitRefDb<'rcx> {
    pub fn new(repo: BitRepo<'rcx>) -> Self {
        Self { repo, bitdir: repo.bitdir }
    }

    pub fn join_ref(&self, path: BitPath) -> BitPath {
        self.bitdir.join(path)
    }

    pub fn join_log(&self, path: BitPath) -> BitPath {
        self.bitdir.join("logs").join(path)
    }

    // tries to expand the symbolic reference
    // i.e. master -> refs/heads/master
    fn try_expand_symref(&self, sym: SymbolicRef) -> Option<SymbolicRef> {
        const PREFIXES: &[BitPath] = &[BitPath::EMPTY, BitPath::REFSHEADS, BitPath::REFSTAGS];
        // we only try to do expansion on single component paths (which all valid branches should be)
        let prefixes =
            if sym.path.as_path().components().count() == 1 { PREFIXES } else { &[BitPath::EMPTY] };
        for prefix in prefixes {
            let path = prefix.join(sym.path);
            if self.join_ref(path).exists() {
                return Some(SymbolicRef::new(path));
            }
        }
        None
    }
}

// unfortunately, doesn't seem like its easy to support a resolve operation on refdb as it will require reading
// objects for validation but both refdb and odb are owned by the repo so not sure if this is feasible
pub trait BitRefDbBackend {
    fn create_branch(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()>;
    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    // may implicitly create the ref
    fn update(&self, sym: SymbolicRef, to: BitRef, cause: RefUpdateCause) -> BitResult<()>;
    fn delete(&self, sym: SymbolicRef) -> BitResult<()>;
    fn exists(&self, sym: SymbolicRef) -> BitResult<bool>;
    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<Filelock<BitReflog>>;

    /// partially resolve means resolves the reference one layer
    /// e.g. HEAD -> refs/heads/master
    fn partially_resolve(&self, reference: BitRef) -> BitResult<BitRef>;

    /// resolves the reference as much as possible.
    /// if the symref points to a path that doesn't exist, then the value of the symref itself is returned.
    /// i.e. if `HEAD` -> `refs/heads/master` which doesn't yet exist, then `refs/heads/master` will be
    /// returned iff a symbolic ref points at a non-existing branch
    fn resolve(&self, reference: BitRef) -> BitResult<BitRef> {
        match self.partially_resolve(reference)? {
            BitRef::Direct(oid) => Ok(BitRef::Direct(oid)),
            // avoid infinite loops as partially resolve may return the reference unchanged
            r @ BitRef::Symbolic(..) if r == reference => Ok(reference),
            BitRef::Symbolic(sym) => self.resolve(BitRef::Symbolic(sym)),
        }
    }

    /// resolves a reference to an oid
    fn fully_resolve(&self, reference: BitRef) -> BitResult<Oid> {
        match self.resolve(reference)? {
            BitRef::Direct(oid) => Ok(oid),
            BitRef::Symbolic(sym) => bail!(BitError::NonExistentSymRef(sym)),
        }
    }

    fn log(
        &self,
        sym: SymbolicRef,
        new_oid: Oid,
        committer: BitSignature,
        msg: String,
    ) -> BitResult<()> {
        // TODO consider caching each reflog that has been read (by holding onto the guard)
        // only necessary if multiple writes will be done successively (such as rebase perhaps)
        self.read_reflog(sym)?.append(new_oid, committer, msg);
        Ok(())
    }
}

impl<'rcx> BitRefDbBackend for BitRefDb<'rcx> {
    fn create_branch(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()> {
        if self.exists(sym)? {
            // todo improve error message by only leaving the branch name in a reliable manner somehow
            // how do we differentiate something that lives in refs/heads vs HEAD
            bail!("a reference `{}` already exists", sym);
        }
        self.update(sym, from, RefUpdateCause::NewBranch { from })
    }

    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        Lockfile::with_readonly(self.join_ref(sym.path), LockfileFlags::SET_READONLY, |lockfile| {
            let head_file =
                lockfile.file().unwrap_or_else(|| panic!("ref `{}` does not exist", sym));
            BitRef::deserialize_unbuffered(head_file)
        })
    }

    fn update(&self, sym: SymbolicRef, to: BitRef, cause: RefUpdateCause) -> BitResult<()> {
        Lockfile::with_mut(self.join_ref(sym.path), LockfileFlags::SET_READONLY, |lockfile| {
            to.serialize(lockfile)
        })?;

        let new_oid = self.repo.fully_resolve_ref(to)?;
        let committer = self.repo.user_signature()?;

        let cause_str = cause.to_string();

        // TODO not sure this is completely correct behaviour, but it at least works for commits
        // if HEAD points to the ref being updated, then we also record the same update in HEAD's log
        if let BitRef::Symbolic(head) = self.repo.read_head()? {
            if head == sym {
                self.log(SymbolicRef::HEAD, new_oid, committer.clone(), cause_str.clone())?;
            }
        }

        self.log(sym, new_oid, committer, cause_str)
    }

    fn delete(&self, _sym: SymbolicRef) -> BitResult<()> {
        todo!()
    }

    fn exists(&self, sym: SymbolicRef) -> BitResult<bool> {
        Ok(self.join_ref(sym.path).exists())
    }

    // read_reflog is probably not a great method to have
    // probably better to have method that directly manipulate the log instead
    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<Filelock<BitReflog>> {
        let path = self.join_log(sym.path);
        Filelock::lock(path)
    }

    fn partially_resolve(&self, reference: BitRef) -> BitResult<BitRef> {
        match reference {
            BitRef::Direct(..) => Ok(reference),
            BitRef::Symbolic(sym) => {
                let repo = self.repo;
                let expanded_sym = match self.try_expand_symref(sym) {
                    Some(expanded) => expanded,
                    None => return Ok(reference),
                };
                let ref_path = self.join_ref(expanded_sym.path);

                let r = Lockfile::with_readonly(ref_path, LockfileFlags::SET_READONLY, |_| {
                    let contents = std::fs::read_to_string(ref_path)?;
                    // symbolic references can be recursive
                    // i.e. HEAD -> refs/heads/master -> <oid>
                    BitRef::from_str(contents.trim_end())
                })?;

                if let BitRef::Direct(oid) = r {
                    ensure!(
                        repo.obj_exists(oid)?,
                        "invalid reference: reference at `{}` which contains invalid object hash `{}` (from symbolic reference `{}`)",
                        ref_path,
                        oid,
                        sym
                    );
                }

                debug!("BitRef::resolve: resolved ref `{:?}` to `{:?}`", sym, r);

                Ok(r)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RefUpdateCommitKind {
    Amend,
    Initial,
    Normal,
}

#[derive(Debug, Clone)]
pub enum RefUpdateCause {
    NewBranch { from: BitRef },
    Commit { subject: String, kind: RefUpdateCommitKind },
}

impl Display for RefUpdateCause {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RefUpdateCause::NewBranch { from } => write!(f, "branch: Created from {}", from),
            RefUpdateCause::Commit { subject, kind } => match *kind {
                RefUpdateCommitKind::Normal => write!(f, "commit: {}", subject),
                RefUpdateCommitKind::Amend => write!(f, "commit (amend): {}", subject),
                RefUpdateCommitKind::Initial => write!(f, "commit (initial): {}", subject),
            },
        }
    }
}
