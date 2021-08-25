use crate::cache::BitObjCache;
use crate::error::{BitError, BitErrorExt, BitGenericError, BitResult};
use crate::index::BitIndex;
use crate::io::ReadExt;
use crate::obj::*;
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::{self, BitPath};
use crate::refs::{BitRef, BitRefDb, BitRefDbBackend, RefUpdateCause, Refs, SymbolicRef};
use crate::rev::Revspec;
use crate::signature::BitSignature;
use crate::{hash, tls};
use anyhow::Context;
use parking_lot::RwLock;
use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::lazy::SyncOnceCell;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub const BIT_INDEX_FILE_PATH: &str = "index";
pub const BIT_HEAD_FILE_PATH: &str = "HEAD";
pub const BIT_CONFIG_FILE_PATH: &str = "config";
pub const BIT_OBJECTS_DIR_PATH: &str = "objects";

#[derive(Copy, Clone)]
pub struct BitRepo<'rcx> {
    rcx: &'rcx RepoCtxt<'rcx>,
}

impl PartialEq for BitRepo<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.rcx, other.rcx)
    }
}

pub struct RepoCtxt<'rcx> {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub workdir: BitPath,
    pub bitdir: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
    odb_cell: SyncOnceCell<BitObjDb>,
    obj_cache: RwLock<BitObjCache<'rcx>>,
    refdb_cell: SyncOnceCell<BitRefDb<'rcx>>,
    index_cell: SyncOnceCell<RwLock<BitIndex<'rcx>>>,
}

pub trait Repo<'rcx> {
    type Odb: BitObjDbBackend;
    type RefDb: BitRefDbBackend<'rcx>;

    fn odb(&self) -> BitResult<&Self::Odb>;
    fn refdb(&self) -> BitResult<&Self::RefDb>;
}

impl<'rcx> Repo<'rcx> for BitRepo<'rcx> {
    type Odb = BitObjDb;
    type RefDb = BitRefDb<'rcx>;

    fn odb(&self) -> BitResult<&Self::Odb> {
        self.odb_cell.get_or_try_init(|| BitObjDb::new(self.bitdir.join(BIT_OBJECTS_DIR_PATH)))
    }

    fn refdb(&self) -> BitResult<&Self::RefDb> {
        self.refdb_cell.get_or_try_init(|| Ok(BitRefDb::new(*self)))
    }
}

impl<'rcx> RepoCtxt<'rcx> {
    fn new(workdir: PathBuf, bitdir: PathBuf, config_filepath: PathBuf) -> BitResult<Self> {
        let workdir = BitPath::intern(workdir);
        let bitdir = BitPath::intern(bitdir);
        let config_filepath = BitPath::intern(config_filepath);
        let index_filepath = bitdir.join(BIT_INDEX_FILE_PATH);

        let this = Self {
            config_filepath,
            workdir,
            bitdir,
            index_filepath,
            odb_cell: Default::default(),
            index_cell: Default::default(),
            obj_cache: Default::default(),
            refdb_cell: Default::default(),
        };

        Ok(this)
    }

    fn find_inner(path: &Path) -> BitResult<Self> {
        if path.join(".git").exists() {
            return Self::load(path);
        }

        // also recognize `.bit` folder as its convenient for having bit repos under tests/repos
        // it is for testing and debugging purposes only
        if path.join(".bit").exists() {
            return Self::load_with_bitdir(path, ".bit");
        }

        match path.parent() {
            Some(parent) => Self::find_inner(parent),
            None => Err(anyhow!("not a bit repository (or any of the parent directories)")),
        }
    }

    fn load_with_bitdir(path: impl AsRef<Path>, bitdir: impl AsRef<Path>) -> BitResult<Self> {
        let worktree = path
            .as_ref()
            .canonicalize()
            .with_context(|| anyhow!("failed to load bit in non-existent directory"))?;
        let bitdir = worktree.join(bitdir);
        debug_assert!(bitdir.exists());
        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);

        let rcx = RepoCtxt::new(worktree, bitdir, config_filepath)?;

        let version = rcx
            .config()
            .repositoryformatversion()?
            .expect("`repositoryformatversion` missing in configuration");

        ensure!(
            version == 0,
            "unsupported repositoryformatversion `{}`, expected version 0",
            version
        );

        Ok(rcx)
    }

    fn load(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::load_with_bitdir(path, ".git")
    }

    pub fn with_res<R>(&'rcx self, f: impl FnOnce(BitRepo<'rcx>) -> BitResult<R>) -> BitResult<R> {
        self.with(f)
    }

    pub fn with<R>(&'rcx self, f: impl FnOnce(BitRepo<'rcx>) -> R) -> R {
        f(BitRepo { rcx: self })
    }

    #[inline]
    pub fn config_path(&self) -> BitPath {
        self.config_filepath
    }

    #[inline]
    pub fn index_path(&self) -> BitPath {
        self.index_filepath
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum RepoState {
    None,
    Merging,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn repo_state(self) -> RepoState {
        if self.bitdir.join(BitPath::MERGE_HEAD).exists() {
            RepoState::Merging
        } else {
            RepoState::None
        }
    }

    /// returns `None` if the reference does not yet exist
    // don't think this can be written in terms of `fully_resolve_ref` below
    // if we were to do something like `fully_resolve_ref().ok()`, then all errors will result in None
    // which is not quite right
    pub fn try_fully_resolve_ref(self, reference: impl Into<BitRef>) -> BitResult<Option<Oid>> {
        match self.resolve_ref(reference)? {
            BitRef::Direct(oid) => Ok(Some(oid)),
            _ => Ok(None),
        }
    }

    pub fn partially_resolve_ref(self, reference: impl Into<BitRef>) -> BitResult<BitRef> {
        self.refdb()?.partially_resolve(reference.into())
    }

    pub fn resolve_ref(self, reference: impl Into<BitRef>) -> BitResult<BitRef> {
        self.refdb()?.resolve(reference.into())
    }

    pub fn fully_resolve_ref(self, reference: impl Into<BitRef>) -> BitResult<Oid> {
        self.refdb()?.fully_resolve(reference.into())
    }

    pub fn default_signature(self) -> BitResult<BitSignature> {
        todo!()
        // BitSignature { name: self.config, email: , time: () }
    }

    /// initialize a repository and use it in the closure
    // testing convenience function
    #[cfg(test)]
    pub fn init_load<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(BitRepo<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::init(&path)?;
        let ctxt = RepoCtxt::load(&path)?;
        tls::enter_repo(&ctxt, f)
    }

    /// recursively searches parents starting from the current directory for a git repo
    pub fn find<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(BitRepo<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        let path = path.as_ref();
        let canonical_path = path.canonicalize().with_context(|| {
            format!("failed to find bit repository in nonexistent path `{}`", path.display())
        })?;
        let ctxt = RepoCtxt::find_inner(canonical_path.as_ref())?;

        tls::enter_repo(&ctxt, f)
    }

    fn index_ref(&self) -> BitResult<&RwLock<BitIndex<'rcx>>> {
        self.index_cell
            .get_or_try_init::<_, BitGenericError>(|| Ok(RwLock::new(BitIndex::new(*self)?)))
    }

    pub fn with_index<R>(self, f: impl FnOnce(&BitIndex<'rcx>) -> BitResult<R>) -> BitResult<R> {
        // don't have to error check here as the index only
        f(&*self.index_ref()?.read())
    }

    pub fn with_index_mut<R>(
        self,
        f: impl FnOnce(&mut BitIndex<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        let index_ref = self.index_ref()?;
        let index = &mut *index_ref.write();
        match f(index) {
            Ok(r) => Ok(r),
            Err(err) => {
                index.rollback();
                Err(err)
            }
        }
    }

    // returns unit as we don't want anyone accessing the repo directly like this
    pub fn init(path: impl AsRef<Path>) -> BitResult<()> {
        let workdir = path.as_ref().canonicalize()?;

        if workdir.is_file() {
            bail!("`{}` is not a directory", workdir.display())
        }

        // `.git` directory not `.bit` as this should be fully compatible with git
        // although, bit will recognize a `.bit` folder if explicitly renamed
        let bitdir = workdir.join(".git");

        if bitdir.exists() {
            // reinitializing doesn't really do anything currently
            println!("reinitialized existing bit repository in `{}`", workdir.display());
            return Ok(());
        }

        std::fs::create_dir(&bitdir)?;

        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);

        RepoCtxt::new(workdir, bitdir, config_filepath)?.with(|repo| {
            repo.mk_bitdir("objects")?;
            repo.mk_bitdir("refs/tags")?;
            repo.mk_bitdir("refs/heads")?;

            let mut desc = repo.mk_bitfile("description")?;
            writeln!(
                desc,
                "Unnamed repository; edit this file 'description' to name the repository."
            )?;

            let mut head = repo.mk_bitfile("HEAD")?;
            writeln!(head, "ref: refs/heads/master")?;

            repo.with_local_config(|config| {
                config.set("core", "repositoryformatversion", 0)?;
                config.set("core", "bare", false)?;
                config.set("core", "filemode", true)?;
                Ok(())
            })?;

            println!("initialized empty bit repository in `{}`", repo.workdir.display());
            Ok(())
        })
    }

    /// todo only works with full hash
    pub fn get_full_object_hash(self, id: BitId) -> BitResult<Oid> {
        match id {
            BitId::Full(hash) => Ok(hash),
            BitId::Partial(_partial) => todo!(),
        }
    }

    /// gets the oid of the tree belonging to the HEAD commit
    /// returns Oid::Unknown if there is no HEAD commit
    pub fn head_tree(self) -> BitResult<Oid> {
        let oid = match self.resolve_head()? {
            BitRef::Direct(oid) => oid,
            _ => return Ok(Oid::UNKNOWN),
        };
        Ok(self.read_obj(oid)?.into_commit().tree)
    }

    /// returns the resolved reference of HEAD
    pub fn resolve_head(self) -> BitResult<BitRef> {
        let head = self.read_head()?;
        self.resolve_ref(head)
    }

    /// returns the resolved oid of HEAD
    pub fn fully_resolve_head(self) -> BitResult<Oid> {
        let head = self.read_head()?;
        self.fully_resolve_ref(head)
    }

    /// reads the contents of `HEAD`
    /// e.g. if `HEAD` -> `ref: refs/heads/master`
    /// then `BitRef::Symbolic(SymbolicRef("refs/heads/master"))` is returned`
    pub fn read_head(self) -> BitResult<BitRef> {
        // another pain point where we have to handle the case where HEAD points at a nonexistent ref
        // `refdb.read(sym)` validates the ref it reads so would fail as it doesn't exist so we just "catch"
        // the error and return that ref anyway
        match self.refdb()?.read(SymbolicRef::HEAD) {
            Ok(r) => Ok(r),
            Err(err) => err.try_into_nonexistent_symref_err().map(Into::into),
        }
    }

    pub fn ls_refs(self) -> BitResult<Refs> {
        self.refdb()?.ls_refs()
    }

    pub fn is_head_detached(self) -> BitResult<bool> {
        Ok(self.read_head()?.is_direct())
    }

    pub fn update_head(self, bitref: impl Into<BitRef>, cause: RefUpdateCause) -> BitResult<()> {
        self.update_ref(SymbolicRef::HEAD, bitref.into(), cause)
    }

    pub fn read_ref(self, sym: SymbolicRef) -> BitResult<BitRef> {
        self.refdb()?.read(sym)
    }

    pub fn validate_ref(self, reference: impl Into<BitRef>) -> BitResult<BitRef> {
        self.refdb()?.validate(reference.into())
    }

    pub fn create_branch(self, sym: SymbolicRef, from: &Revspec) -> BitResult<()> {
        // we fully resolve the reference to an oid and write that into the new branch file
        self.refdb()?.create_branch(sym, from)
    }

    pub fn update_ref(
        self,
        sym: SymbolicRef,
        to: impl Into<BitRef>,
        cause: RefUpdateCause,
    ) -> BitResult<()> {
        self.refdb()?.update(sym, to.into(), cause)
    }

    /// writes `obj` into the object store returning its full hash
    pub fn write_obj(self, obj: &dyn WritableObject) -> BitResult<Oid> {
        self.odb()?.write(obj)
    }

    pub fn read_obj(self, id: impl Into<BitId>) -> BitResult<BitObjKind<'rcx>> {
        let oid = self.expand_id(id)?;
        self.obj_cache.write().get_or_insert_with(oid, || {
            let raw = self.odb()?.read_raw(BitId::Full(oid))?;
            BitObjKind::from_raw(self, raw)
        })
    }

    pub fn expand_id(self, id: impl Into<BitId>) -> BitResult<Oid> {
        self.odb()?.expand_id(id.into())
    }

    pub fn expand_prefix(self, prefix: PartialOid) -> BitResult<Oid> {
        self.odb()?.expand_prefix(prefix)
    }

    pub fn ensure_obj_exists(self, id: impl Into<BitId>) -> BitResult<()> {
        let id = id.into();
        ensure!(self.odb()?.exists(id)?, BitError::ObjectNotFound(id));
        Ok(())
    }

    #[must_use = "this call has no side effects (you may want to use `ensure_obj_exists` instead)"]
    // note, the above annotation doesn't really do anything as "question marking" the return value counts as a use so...
    // but nevertheless, its non-useless docs as I've made the mistake already
    pub fn obj_exists(self, id: impl Into<BitId>) -> BitResult<bool> {
        self.odb()?.exists(id.into())
    }

    pub fn read_obj_header(self, id: impl Into<BitId>) -> BitResult<BitObjHeader> {
        self.odb()?.read_header(id.into())
    }

    /// Read the file at `path` on the worktree into a mutable blob object
    pub fn read_blob_from_worktree(self, path: impl AsRef<Path>) -> BitResult<MutableBlob> {
        let path = self.normalize_path(path.as_ref())?;
        let bytes = if path.symlink_metadata()?.file_type().is_symlink() {
            // we literally hash the contents of the symlink without following
            std::fs::read_link(path)?.as_os_str().as_bytes().to_vec()
        } else {
            File::open(path)?.read_to_vec()?
        };
        Ok(MutableBlob::new(bytes))
    }

    /// Get the blob at `path` on the worktree and return its hash
    pub fn hash_blob_from_worktree(self, path: BitPath) -> BitResult<Oid> {
        self.read_blob_from_worktree(path).and_then(|blob| hash::hash_obj(&blob))
    }

    /// convert a relative path to be absolute based off the repository root
    /// use [`Self::normalize_path`] if you expect the path to exist
    pub fn to_absolute_path(self, path: impl AsRef<Path>) -> BitPath {
        self.workdir.join(path)
    }

    /// converts relative_paths to absolute paths
    /// checks absolute paths exist and have a base relative to the bit directory
    // can't figure out how to make this take an impl AsRef<Path> and make lifetimes work out
    pub fn normalize_path<'p>(self, path: &Path) -> BitResult<Cow<'_, Path>> {
        // `self.worktree` should be a canonical, absolute path
        // and path should be relative to it, so we can just join them
        debug_assert!(self.workdir.is_absolute());
        if path.is_relative() {
            let normalized = path::normalize(&self.to_absolute_path(&path));
            debug_assert!(
                normalized.symlink_metadata().is_ok(),
                "normalized path `{}` does not exist",
                normalized.display()
            );
            Ok(Cow::Owned(normalized))
        } else {
            debug_assert!(
                path.starts_with(&self.workdir),
                "absolute path `{}` is not under current bit directory `{}`",
                path.display(),
                self.workdir
            );
            Ok(Cow::Borrowed(path))
        }
    }

    /// converts an absolute path into a path relative to the workdir of the repository
    pub fn to_relative_path(self, path: &Path) -> BitResult<&Path> {
        // this seems to work just as well as the pathdiff crate
        debug_assert!(path.is_absolute());
        Ok(path.strip_prefix(&self.workdir)?)
    }

    #[cfg(test)]
    pub(crate) fn relative_paths(self, paths: &[impl AsRef<Path>]) -> BitPath {
        paths.iter().fold(self.bitdir, |base, path| base.join(path))
    }

    pub(crate) fn mk_bitdir(self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.bitdir.join(path))
    }

    pub(crate) fn mk_bitfile(self, path: impl AsRef<Path>) -> io::Result<File> {
        File::create(self.bitdir.join(path))
    }
}

impl Debug for BitRepo<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitRepo")
            .field("worktree", &self.workdir)
            .field("bitdir", &self.bitdir)
            .finish_non_exhaustive()
    }
}

impl<'rcx> Deref for BitRepo<'rcx> {
    type Target = RepoCtxt<'rcx>;

    fn deref(&self) -> &Self::Target {
        self.rcx
    }
}

#[cfg(test)]
mod tests;
