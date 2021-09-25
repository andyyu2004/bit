use crate::cache::{BitObjCache, VirtualOdb};
use crate::config::{BitConfig, RemoteConfig};
use crate::error::{BitError, BitErrorExt, BitGenericError, BitResult};
use crate::index::BitIndex;
use crate::io::ReadExt;
use crate::merge::MergeStrategy;
use crate::obj::*;
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::{self, BitPath};
use crate::refs::{BitRef, BitRefDb, BitRefDbBackend, RefUpdateCause, Refs, SymbolicRef};
use crate::rev::Revspec;
use crate::signature::BitSignature;
use crate::tls;
use anyhow::Context;
use bit_ds::sync::OneThread;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::future::Future;
use std::io::{self, Write};
use std::lazy::SyncOnceCell;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use typed_arena::Arena as TypedArena;

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

#[derive(Default)]
pub struct Arenas<'rcx> {
    commit_arena: TypedArena<Commit<'rcx>>,
    tree_arena: TypedArena<Tree<'rcx>>,
    blob_arena: TypedArena<Blob<'rcx>>,
    tag_arena: TypedArena<Tag<'rcx>>,
}

/// The context backing `BitRepo`
/// Most of the fields are using threadsafe wrappers due to experimentation with
/// multithreading but the results were not great so they are not in use currently.
pub struct RepoCtxt<'rcx> {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub workdir: BitPath,
    pub bitdir: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
    arenas: OneThread<Arenas<'rcx>>,
    config: BitConfig,
    obj_cache: RwLock<BitObjCache<'rcx>>,
    odb_cell: SyncOnceCell<BitObjDb>,
    refdb_cell: SyncOnceCell<BitRefDb<'rcx>>,
    index_cell: SyncOnceCell<RwLock<BitIndex<'rcx>>>,
    virtual_odb: SyncOnceCell<VirtualOdb<'rcx>>,
    virtual_write: AtomicBool,
}

impl<'rcx> RepoCtxt<'rcx> {
    fn new(workdir: PathBuf, bitdir: PathBuf, config_filepath: PathBuf) -> BitResult<Self> {
        let workdir = BitPath::intern(workdir);
        let bitdir = BitPath::intern(bitdir);
        let config_filepath = BitPath::intern(config_filepath);
        let config = BitConfig::init(config_filepath)?;
        let index_filepath = bitdir.join(BIT_INDEX_FILE_PATH);

        let this = Self {
            config_filepath,
            workdir,
            bitdir,
            index_filepath,
            config,
            arenas: OneThread::new(Default::default()),
            odb_cell: Default::default(),
            index_cell: Default::default(),
            obj_cache: Default::default(),
            refdb_cell: Default::default(),
            virtual_odb: Default::default(),
            virtual_write: Default::default(),
        };

        Ok(this)
    }

    #[inline]
    pub fn config(&self) -> &BitConfig {
        &self.config
    }

    fn find_inner(path: &Path) -> BitResult<Self> {
        if path.join(".git").try_exists()? {
            return Self::load(path);
        }

        // also recognize `.bit` folder as its convenient for having bit repos under tests/repos
        // it is for testing and debugging purposes only
        if path.join(".bit").try_exists()? {
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
        debug_assert!(bitdir.try_exists()?);
        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);

        let rcx = RepoCtxt::new(worktree, bitdir, config_filepath)?;

        let version = rcx
            .config()
            .repositoryformatversion()
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

    pub async fn with_async<F, R>(&'rcx self, f: impl FnOnce(BitRepo<'rcx>) -> F) -> R
    where
        F: Future<Output = R>,
    {
        f(BitRepo { rcx: self }).await
    }

    /// Enter the repository context for a "transaction".
    /// If any fatal error occurs during the closure, then writes to the index are rolled back.
    /// This should generally be used as the entry point to accessing the repository
    pub fn enter<R>(&'rcx self, f: impl FnOnce(BitRepo<'rcx>) -> BitResult<R>) -> BitResult<R> {
        tls::enter_repo(self, |repo| match f(repo) {
            Ok(r) => Ok(r),
            Err(err) => {
                if repo.index_cell.get().is_some() {
                    repo.index_lock()?.write().rollback();
                }
                Err(err)
            }
        })
    }

    pub async fn enter_async<F, R>(&'rcx self, f: impl FnOnce(BitRepo<'rcx>) -> F) -> BitResult<R>
    where
        F: Future<Output = BitResult<R>>,
    {
        self.with_async(async move |repo| match f(repo).await {
            Ok(r) => Ok(r),
            Err(err) => {
                if repo.index_cell.get().is_some() {
                    repo.index_lock()?.write().rollback();
                }
                Err(err)
            }
        })
        .await
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

    #[inline]
    pub fn config(self) -> &'rcx BitConfig {
        self.rcx.config()
    }

    #[inline]
    pub fn remote_config(self) -> &'rcx HashMap<&'static str, RemoteConfig> {
        &self.rcx.config().remote.remotes
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
        RepoCtxt::load(&path)?.enter(f)
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
        RepoCtxt::find_inner(canonical_path.as_ref())?.enter(f)
    }

    #[inline]
    pub fn refdb(&self) -> BitResult<&BitRefDb<'rcx>> {
        self.refdb_cell.get_or_try_init(|| Ok(BitRefDb::new(*self)))
    }

    // this is necessary as a lifetime hint as otherwise usages of &self.arenas have lifetime
    // tied to self rather than 'rcx
    #[inline]
    fn arenas(self) -> &'rcx Arenas<'rcx> {
        &self.rcx.arenas
    }

    #[inline]
    pub(crate) fn cache(self) -> &'rcx RwLock<BitObjCache<'rcx>> {
        &self.rcx.obj_cache
    }

    #[inline]
    pub(crate) fn alloc_commit(self, commit: Commit<'rcx>) -> &'rcx Commit<'rcx> {
        self.arenas().commit_arena.alloc(commit)
    }

    #[inline]
    pub(crate) fn alloc_tree(self, tree: Tree<'rcx>) -> &'rcx Tree<'rcx> {
        self.arenas().tree_arena.alloc(tree)
    }

    #[inline]
    pub(crate) fn alloc_blob(self, blob: Blob<'rcx>) -> &'rcx Blob<'rcx> {
        self.arenas().blob_arena.alloc(blob)
    }

    #[inline]
    pub(crate) fn alloc_tag(self, tag: Tag<'rcx>) -> &'rcx Tag<'rcx> {
        self.arenas().tag_arena.alloc(tag)
    }

    #[inline]
    fn index_lock(self) -> BitResult<&'rcx RwLock<BitIndex<'rcx>>> {
        self.rcx
            .index_cell
            .get_or_try_init::<_, BitGenericError>(|| Ok(RwLock::new(BitIndex::new(self)?)))
    }

    #[inline]
    pub fn index(self) -> BitResult<RwLockReadGuard<'rcx, BitIndex<'rcx>>> {
        Ok(self
            .index_lock()?
            .try_read()
            .expect("trying to read index when something is already writing"))
    }

    /// Grant mutable access to the index
    /// Be very cautious and hold the lock for the shortest period possible as it's fairly easy to run into deadlocks.
    /// Don't just declare index at the top of the function.
    #[inline]
    pub fn index_mut(self) -> BitResult<RwLockWriteGuard<'rcx, BitIndex<'rcx>>> {
        Ok(self
            .index_lock()?
            .try_write()
            .expect("concurrent writes to the index shouldn't be possible atm; probably tried to acquire this reentrantly"))
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

        if bitdir.try_exists()? {
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

            File::create(repo.config_path())?;
            repo.with_raw_local_config(|config| {
                config.set("core", "repositoryformatversion", 0)?;
                config.set("core", "bare", false)?;
                config.set("core", "filemode", true)
            })?;

            println!("initialized empty bit repository in `{}`", repo.workdir.display());
            Ok(())
        })
    }

    /// gets the oid of the tree belonging to the HEAD commit
    /// returns Oid::Unknown if there is no HEAD commit
    pub fn head_tree(self) -> BitResult<Oid> {
        match self.resolve_head()? {
            BitRef::Direct(oid) => oid,
            _ => return Ok(Oid::UNKNOWN),
        }
        .treeish_oid(self)
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

    // Update the "current branch" to point at `target`.
    // If currently in detached head state, then HEAD will be updated.
    // Otherwise, the branch pointed to by HEAD will be updated.
    pub fn update_current_ref(self, target: Oid, cause: RefUpdateCause) -> BitResult<()> {
        // does not handle multiple level symrefs but unsure when they arise
        match self.read_head()? {
            BitRef::Direct(..) => self.update_ref(SymbolicRef::HEAD, target, cause),
            BitRef::Symbolic(current_branch) => self.update_ref(current_branch, target, cause),
        }
    }

    pub(crate) fn update_current_ref_for_reset(self, target: impl Into<BitRef>) -> BitResult<()> {
        let target = target.into();
        let oid = self.fully_resolve_ref(target)?;
        self.update_current_ref(oid, RefUpdateCause::Reset { target })
    }

    pub(crate) fn update_current_ref_for_ff_merge(self, to: impl Into<BitRef>) -> BitResult<()> {
        let to = to.into();
        let oid = self.fully_resolve_ref(to)?;
        self.update_current_ref(
            oid,
            RefUpdateCause::Merge { to, strategy: MergeStrategy::FastForward },
        )
    }

    pub(crate) fn update_current_ref_for_merge(self, to: impl Into<BitRef>) -> BitResult<()> {
        let to = to.into();
        let oid = self.fully_resolve_ref(to)?;
        self.update_current_ref(
            oid,
            RefUpdateCause::Merge { to, strategy: MergeStrategy::Recursive },
        )
    }

    pub(crate) fn update_head_for_checkout(self, to: impl Into<BitRef>) -> BitResult<()> {
        let to = to.into();
        self.update_head(to, RefUpdateCause::Checkout { from: self.read_head()?, to })
    }

    /// Enter a section where writes don't persist to disk but only to the cache.
    /// Useful for ephemeral writes (such as virtual merge bases).
    /// Be careful as all writes and reads within the closure will be issued to the virtual odb
    pub(crate) fn with_virtual_write<R>(self, f: impl FnOnce() -> R) -> R {
        self.virtual_write.store(true, Ordering::Release);
        let ret = f();
        self.virtual_write.store(false, Ordering::Release);
        ret
    }

    // this method must be private to avoid people writing directly the odb directly bypassing the `virtual_write` checks
    #[inline]
    fn odb(&self) -> BitResult<&BitObjDb> {
        self.odb_cell.get_or_try_init(|| BitObjDb::new(self.bitdir.join(BIT_OBJECTS_DIR_PATH)))
    }

    fn virtual_odb(self) -> &'rcx VirtualOdb<'rcx> {
        self.rcx.virtual_odb.get_or_init(|| VirtualOdb::new(self))
    }

    /// writes `obj` into the object store returning its full hash
    pub fn write_obj(self, obj: &dyn WritableObject) -> BitResult<Oid> {
        if self.virtual_write.load(Ordering::Acquire) {
            self.virtual_odb().write(obj)
        } else {
            // TODO cache this object as a write is often followed by an immediate read
            // is there even an easy way to cache this write without just deserializing it from scratch essentially?
            self.odb()?.write(obj)
        }
    }

    pub fn read_obj(self, id: impl Into<BitId>) -> BitResult<BitObjKind<'rcx>> {
        let oid = self.expand_id(id)?;
        if oid == Oid::EMPTY_TREE {
            Ok(BitObjKind::Tree(Tree::empty(self)))
        } else if self.virtual_write.load(Ordering::Acquire) {
            Ok(self.virtual_odb().read(oid))
        } else {
            self.obj_cache.write().get_or_insert_with(oid, || {
                let raw = self.odb()?.read_raw(BitId::Full(oid))?;
                BitObjKind::from_raw(self, raw)
            })
        }
    }

    pub fn read_obj_tree(self, id: impl Into<BitId>) -> BitResult<&'rcx Tree<'rcx>> {
        self.read_obj(id).map(|obj| obj.into_tree())
    }

    pub fn read_obj_commit(self, id: impl Into<BitId>) -> BitResult<&'rcx Commit<'rcx>> {
        self.read_obj(id).map(|obj| obj.into_commit())
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

    pub fn ensure_obj_is_commit(self, id: impl Into<BitId>) -> BitResult<()> {
        let id = id.into();
        let oid = self.expand_id(id)?;
        self.ensure_obj_exists(oid)?;
        let obj_type = self.read_obj_header(oid)?.obj_type;
        ensure!(obj_type == BitObjType::Commit, BitError::ExpectedCommit(oid, obj_type));
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
    pub(crate) fn read_blob_from_worktree(self, path: impl AsRef<Path>) -> BitResult<MutableBlob> {
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
    pub(crate) fn hash_blob_from_worktree(self, path: impl AsRef<Path>) -> BitResult<Oid> {
        let path = path.as_ref();
        debug_assert!(!path.is_dir());
        self.read_blob_from_worktree(path).and_then(|blob| blob.hash())
    }

    /// convert a relative path to be absolute based off the repository root
    /// use [`Self::normalize_path`] if you expect the path to exist
    pub(crate) fn to_absolute_path(self, path: impl AsRef<Path>) -> BitPath {
        self.workdir.join(path)
    }

    /// converts relative_paths to absolute paths
    /// checks absolute paths exist and have a base relative to the bit directory
    // can't figure out how to make this take an impl AsRef<Path> and make lifetimes work out
    pub(crate) fn normalize_path(self, path: &Path) -> BitResult<Cow<'_, Path>> {
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

    #[inline]
    pub(crate) fn mk_bitdir(self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.bitdir.join(path))
    }

    #[inline]
    pub(crate) fn mkdir(self, path: BitPath) -> BitResult<()> {
        Ok(std::fs::create_dir(self.to_absolute_path(path))?)
    }

    #[inline]
    pub(crate) fn rmdir_all(self, path: impl AsRef<Path>) -> BitResult<()> {
        let path = self.to_absolute_path(path.as_ref());
        std::fs::remove_dir_all(path).with_context(|| anyhow!("failed to rmdir_all `{}`", path))
    }

    #[inline]
    pub(crate) fn touch(self, path: impl AsRef<Path>) -> BitResult<std::fs::File> {
        let path = self.to_absolute_path(path.as_ref());
        debug_assert!(!path.try_exists()?);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::File::with_options()
            .create_new(true)
            .write(true)
            .read(false)
            .open(path)
            .with_context(|| anyhow!("failed to touch `{}`", path.display()))
    }

    #[inline]
    pub(crate) fn path_exists(self, path: impl AsRef<Path>) -> io::Result<bool> {
        self.to_absolute_path(path).try_exists()
    }

    #[inline]
    pub(crate) fn rm(self, path: BitPath) -> BitResult<()> {
        std::fs::remove_file(self.to_absolute_path(path))
            .with_context(|| anyhow!("failed to rm `{}`", path))
    }

    #[inline]
    pub(crate) fn mv(self, from: BitPath, to: impl AsRef<Path>) -> BitResult<()> {
        let to = to.as_ref();
        debug_assert!(!to.try_exists()?);
        std::fs::rename(self.to_absolute_path(from), self.to_absolute_path(to))
            .with_context(|| anyhow!("failed to mv `{}` to `{}", from, to.display()))
    }

    #[inline]
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
