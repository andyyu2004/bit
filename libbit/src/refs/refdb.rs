use super::{BitRef, BitReflog, SymbolicRef};
use crate::error::BitResult;
use crate::lockfile::Lockfile;
use crate::path::BitPath;
use crate::serialize::Deserialize;
use crate::serialize::Serialize;

pub struct BitRefDb {
    bitdir: BitPath,
}

impl BitRefDb {
    pub fn new(bitdir: BitPath) -> Self {
        Self { bitdir }
    }

    pub fn join_ref(&self, path: BitPath) -> BitPath {
        self.bitdir.join(path)
    }

    pub fn join_log(&self, path: BitPath) -> BitPath {
        self.bitdir.join("logs").join(path)
    }
}

// unfortunately, doesn't seem like its easy to support a resolve operation on refdb as it will require reading
// objects for validation but both refdb and odb are owned by the repo so not sure if this is feasible
pub trait BitRefDbBackend {
    fn create(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()>;
    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef>;
    // may implicitly create the ref
    fn update(&self, sym: SymbolicRef, to: BitRef) -> BitResult<()>;
    fn delete(&self, sym: SymbolicRef) -> BitResult<()>;
    fn exists(&self, sym: SymbolicRef) -> BitResult<bool>;

    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<BitReflog>;
}

impl BitRefDbBackend for BitRefDb {
    fn create(&self, sym: SymbolicRef, from: BitRef) -> BitResult<()> {
        if self.exists(sym)? {
            // todo improve error message by only leaving the branch name in a reliable manner somehow
            // how do we differentiate something that lives in refs/heads vs HEAD
            bail!("a reference `{}` already exists", sym);
        }
        self.update(sym, from)
    }

    fn read(&self, sym: SymbolicRef) -> BitResult<BitRef> {
        Lockfile::with_readonly(self.join_ref(sym.path), |lockfile| {
            let head_file =
                lockfile.file().unwrap_or_else(|| panic!("ref `{}` does not exist", sym));
            BitRef::deserialize_unbuffered(head_file)
        })
    }

    fn update(&self, sym: SymbolicRef, to: BitRef) -> BitResult<()> {
        Lockfile::with_mut(self.join_ref(sym.path), |lockfile| to.serialize(lockfile))
    }

    fn delete(&self, _sym: SymbolicRef) -> BitResult<()> {
        todo!()
    }

    fn exists(&self, sym: SymbolicRef) -> BitResult<bool> {
        Ok(self.join_ref(sym.path).exists())
    }

    fn read_reflog(&self, sym: SymbolicRef) -> BitResult<BitReflog> {
        let path = self.join_log(sym.path);
        todo!()
    }
}
