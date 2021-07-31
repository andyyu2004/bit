use super::{BitRef, BitReflog, SymbolicRef};
use crate::error::{BitError, BitErrorExt, BitResult};
use crate::lockfile::{Filelock, Lockfile, LockfileFlags};
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::{Deserialize, Serialize};
use crate::signature::BitSignature;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

pub struct BitRefDb<'rcx> {
    repo: BitRepo<'rcx>,
    bitdir: BitPath,
}

impl<'rcx> BitRefDb<'rcx> {
    pub fn new(repo: BitRepo<'rcx>) -> Self {
        Self { repo, bitdir: repo.bitdir }
    }

    pub fn join(&self, path: impl AsRef<Path>) -> BitPath {
        self.bitdir.join(path)
    }

    pub fn join_log(&self, path: BitPath) -> BitPath {
        self.bitdir.join("logs").join(path)
    }
}

pub type Refs = BTreeSet<SymbolicRef>;

// unfortunately, doesn't seem like its easy to support a resolve operation on refdb as it will require reading
// objects for validation but both refdb and odb are owned by the repo so not sure if this is feasible
pub trait BitRefDbBackend<'rcx> {
    fn repo(&self) -> BitRepo<'rcx>;
    fn create_branch(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()>;
    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    // may implicitly create the ref
    fn update(&self, sym: SymbolicRef, to: BitRef, cause: RefUpdateCause) -> BitResult<()>;
    fn delete(&self, sym: SymbolicRef) -> BitResult<()>;
    fn exists(&self, sym: SymbolicRef) -> BitResult<bool>;
    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<Filelock<BitReflog>>;
    // return path of all symbolic refs, branches, and tags
    fn ls_refs(&self) -> BitResult<Refs>;
    // tries to expand the symbolic reference
    // i.e. master -> refs/heads/master
    fn expand_symref(&self, sym: SymbolicRef) -> BitResult<SymbolicRef>;

    /// validate and expand the reference into it's full path
    fn validate(&self, reference: BitRef) -> BitResult<BitRef> {
        match reference {
            BitRef::Direct(oid) => {
                self.repo().ensure_obj_exists(oid)?;
                Ok(reference)
            }
            BitRef::Symbolic(sym) => self.expand_symref(sym).map(BitRef::Symbolic),
        }
    }

    /// partially resolve means resolves the reference one layer
    /// e.g. HEAD -> refs/heads/master
    fn partially_resolve(&self, reference: BitRef) -> BitResult<BitRef> {
        match reference {
            BitRef::Direct(oid) => {
                self.repo().ensure_obj_exists(oid)?;
                Ok(BitRef::Direct(oid))
            }
            BitRef::Symbolic(sym) => {
                let expanded_sym = self.expand_symref(sym)?;
                let validated = self.validate(self.read(expanded_sym)?)?;
                debug!("BitRef::resolve: resolved ref `{:?}` to `{:?}`", sym, validated);
                Ok(validated)
            }
        }
    }

    /// resolves the reference as much as possible.
    /// if the symref points to a path that doesn't exist, then the value of the symref itself is returned.
    /// i.e. if `HEAD` -> `refs/heads/master` which doesn't yet exist, then `refs/heads/master` will be
    /// returned iff a symbolic ref points at a non-existing branch
    fn resolve(&self, reference: BitRef) -> BitResult<BitRef> {
        match self.partially_resolve(reference) {
            Ok(partial) => match partial {
                BitRef::Direct(..) => Ok(partial),
                BitRef::Symbolic(sym) => self.resolve(BitRef::Symbolic(sym)),
            },
            // if partial resolution failed on a symref, then we return that symbolic reference
            // e.g. sym = refs/heads/master, but that file doesn't exist
            // otherwise, propogate the error
            Err(err) => err.try_into_nonexistent_symref_err().map(BitRef::Symbolic),
        }
    }

    /// resolves a reference to an oid
    fn fully_resolve(&self, reference: BitRef) -> BitResult<Oid> {
        match self.resolve(reference)? {
            BitRef::Direct(oid) => Ok(oid),
            BitRef::Symbolic(..) => unreachable!("resolve should never return this variant"),
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

impl<'rcx> BitRefDbBackend<'rcx> for BitRefDb<'rcx> {
    #[inline]
    fn repo(&self) -> BitRepo<'rcx> {
        self.repo
    }

    fn create_branch(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()> {
        if self.exists(sym)? {
            // todo improve error message by only leaving the branch name in a reliable manner somehow
            // how do we differentiate something that lives in refs/heads vs HEAD
            bail!("a reference `{}` already exists", sym);
        }
        self.update(sym, from, RefUpdateCause::NewBranch { from })
    }

    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        Lockfile::with_readonly(self.join(sym.path), LockfileFlags::SET_READONLY, |lockfile| {
            let file = lockfile.file().unwrap_or_else(|| panic!("ref `{}` does not exist", sym));
            BitRef::deserialize_unbuffered(file)
        })
    }

    fn update(&self, sym: SymbolicRef, to: BitRef, cause: RefUpdateCause) -> BitResult<()> {
        Lockfile::with_mut(self.join(sym.path), LockfileFlags::SET_READONLY, |lockfile| {
            self.validate(to)?.serialize(lockfile)
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
        Ok(self.join(sym.path).exists())
    }

    // read_reflog is probably not a great method to have
    // probably better to have method that directly manipulate the log instead
    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<Filelock<BitReflog>> {
        let expanded = self.expand_symref(sym)?;
        let path = self.join_log(expanded.path);
        Filelock::lock(path)
    }

    fn ls_refs(&self) -> BitResult<Refs> {
        let mut refs = btreeset! { SymbolicRef::HEAD };
        let refs_dir = self.join("refs");
        for entry in walkdir::WalkDir::new(refs_dir) {
            let entry = entry?;
            if entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path();
            let sym = SymbolicRef::intern_valid(path.strip_prefix(self.bitdir)?)?;
            assert!(refs.insert(sym), "inserted duplicate ref `{}`", sym);
        }
        Ok(refs)
    }

    // tries to expand the symbolic reference
    // i.e. master -> refs/heads/master
    fn expand_symref(&self, sym: SymbolicRef) -> BitResult<SymbolicRef> {
        const PREFIXES: &[BitPath] =
            &[BitPath::EMPTY, BitPath::REFS_HEADS, BitPath::REFS_TAGS, BitPath::REFS_REMOTES];
        // we only try to do expansion on single component paths (which all valid branches should be)
        let prefixes =
            if sym.path.as_path().components().count() == 1 { PREFIXES } else { &[BitPath::EMPTY] };

        for prefix in prefixes {
            let path = prefix.join(sym.path);
            if self.join(path).exists() {
                return SymbolicRef::new_valid(path);
            }
        }

        bail!(BitError::NonExistentSymRef(sym))
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
    Checkout { from: BitRef, to: BitRef },
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
            RefUpdateCause::Checkout { from, to } =>
                write!(f, "checkout: moving from `{}` to `{}`", from, to),
        }
    }
}
