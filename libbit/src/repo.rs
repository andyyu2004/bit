use crate::error::{BitGenericError, BitResult};
use crate::index::BitIndex;
use crate::io::ReadExt;
use crate::obj::*;
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::{self, BitPath};
use crate::refs::{BitRef, BitRefDb, BitRefDbBackend, RefUpdateCause, SymbolicRef};
use crate::signature::BitSignature;
use crate::{hash, tls};
use anyhow::Context;
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::lazy::OnceCell;
use std::ops::Deref;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub const BIT_INDEX_FILE_PATH: &str = "index";
pub const BIT_HEAD_FILE_PATH: &str = "HEAD";
pub const BIT_CONFIG_FILE_PATH: &str = "config";
pub const BIT_OBJECTS_DIR_PATH: &str = "objects";

#[derive(Copy, Clone)]
pub struct BitRepo<'rcx> {
    ctxt: &'rcx RepoCtxt<'rcx>,
}

pub struct RepoCtxt<'rcx> {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub workdir: BitPath,
    pub bitdir: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
    odb_cell: OnceCell<BitObjDb>,
    refdb_cell: OnceCell<BitRefDb<'rcx>>,
    index_cell: OnceCell<RefCell<BitIndex<'rcx>>>,
}

pub trait Repo<'rcx> {
    type Odb: BitObjDbBackend;
    type RefDb: BitRefDbBackend;

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
            refdb_cell: Default::default(),
        };

        Ok(this)
    }

    fn find_inner(path: &Path) -> BitResult<Self> {
        if path.join(".git").exists() {
            return Self::load(path);
        }

        // also recognize `.bit` folder as its convenient for having
        // bit repos under tests/repos
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
        f(BitRepo { ctxt: self })
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

impl<'rcx> BitRepo<'rcx> {
    /// returns `None` if the reference does not yet exist
    // don't think this can be written in terms of `fully_resolve_ref` below
    // if we were to do something like `fully_resolve_ref().ok()`, then all errors will result in None
    // which is not quite right
    pub fn try_fully_resolve_ref(self, reference: impl Into<BitRef>) -> BitResult<Option<Oid>> {
        match self.resolve_ref(reference)? {
            BitRef::Direct(oid) => Ok(Some(oid)),
            BitRef::Symbolic(_) => Ok(None),
        }
    }

    pub fn partially_resolve_ref(&self, reference: impl Into<BitRef>) -> BitResult<BitRef> {
        self.refdb()?.partially_resolve(reference.into())
    }

    pub fn resolve_ref(&self, reference: impl Into<BitRef>) -> BitResult<BitRef> {
        self.refdb()?.resolve(reference.into())
    }

    pub fn fully_resolve_ref(&self, reference: impl Into<BitRef>) -> BitResult<Oid> {
        self.refdb()?.fully_resolve(reference.into())
    }

    pub fn default_signature(&self) -> BitResult<BitSignature> {
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

    fn index_ref(&self) -> BitResult<&RefCell<BitIndex<'rcx>>> {
        self.index_cell
            .get_or_try_init::<_, BitGenericError>(|| Ok(RefCell::new(BitIndex::new(*self)?)))
    }

    pub fn with_index<R>(self, f: impl FnOnce(&BitIndex<'_>) -> BitResult<R>) -> BitResult<R> {
        // don't have to error check here as the index only
        f(&*self.index_ref()?.borrow())
    }

    pub fn with_index_mut<R>(
        self,
        f: impl FnOnce(&mut BitIndex<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        let index_ref = self.index_ref()?;
        let index = &mut *index_ref.borrow_mut();
        match f(index) {
            Ok(r) => Ok(r),
            Err(err) => {
                index.rollback();
                Err(err)
            }
        }
        // Lockfile::with_mut(self.index_path(), LockfileFlags::SET_READONLY, |lockfile| {
        //     let index = &mut BitIndex::from_lockfile(self, &lockfile)?;
        //     let r = f(index)?;
        //     if index.flags.contains(BitIndexFlags::DIRTY) {
        //         index.serialize(lockfile)?;
        //     } else {
        //         lockfile.rollback();
        //     }
        //     Ok(r)
        // })
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
    pub fn get_full_object_hash(&self, id: BitId) -> BitResult<Oid> {
        match id {
            BitId::Full(hash) => Ok(hash),
            BitId::Partial(_partial) => todo!(),
        }
    }

    /// gets the oid of the tree belonging to the HEAD commit
    /// returns Oid::Unknown if there is no HEAD commit
    pub fn head_tree(&self) -> BitResult<Oid> {
        let oid = match self.resolve_head()? {
            BitRef::Direct(oid) => oid,
            _ => return Ok(Oid::UNKNOWN),
        };
        Ok(self.read_obj(oid)?.into_commit().tree)
    }

    /// returns the resolved hash of the HEAD symref
    pub fn resolve_head(&self) -> BitResult<BitRef> {
        let head = self.read_head()?;
        self.resolve_ref(head)
    }

    /// reads the contents of `HEAD`
    /// e.g. if `HEAD` -> `ref: refs/heads/master`
    /// then `BitRef::Symbolic(SymbolicRef("refs/heads/master"))` is returned`
    pub fn read_head(&self) -> BitResult<BitRef> {
        self.refdb()?.read(SymbolicRef::HEAD)
    }

    pub fn update_head(&self, bitref: impl Into<BitRef>, cause: RefUpdateCause) -> BitResult<()> {
        self.update_ref(SymbolicRef::HEAD, bitref.into(), cause)
    }

    pub fn create_branch(self, sym: SymbolicRef, from: SymbolicRef) -> BitResult<()> {
        // we fully resolve the reference to an oid and write that into the new branch file
        let resolved = self.fully_resolve_ref(from)?;
        self.refdb()?.create_branch(sym, resolved.into())
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
    pub fn write_obj(&self, obj: &dyn WritableObject) -> BitResult<Oid> {
        self.odb()?.write(obj)
    }

    pub fn read_obj(&self, id: impl Into<BitId>) -> BitResult<BitObjKind> {
        self.odb()?.read(id.into())
    }

    pub fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid> {
        self.odb()?.expand_prefix(prefix)
    }

    pub fn obj_exists(&self, id: impl Into<BitId>) -> BitResult<bool> {
        self.odb()?.exists(id.into())
    }

    pub fn read_obj_header(&self, id: impl Into<BitId>) -> BitResult<BitObjHeader> {
        self.odb()?.read_header(id.into())
    }

    pub fn get_blob(&self, path: BitPath) -> BitResult<MutableBlob> {
        let path = self.normalize(path.as_ref())?;
        let bytes = if path.symlink_metadata()?.file_type().is_symlink() {
            // we literally hash the contents of the symlink without following
            std::fs::read_link(path)?.as_os_str().as_bytes().to_vec()
        } else {
            File::open(path)?.read_to_vec()?
        };
        Ok(MutableBlob::new(bytes))
    }

    pub fn write_blob(&self, path: BitPath) -> BitResult<Oid> {
        let blob = self.get_blob(path)?;
        self.write_obj(&blob)
    }

    pub fn hash_blob(&self, path: BitPath) -> BitResult<Oid> {
        self.get_blob(path).and_then(|blob| hash::hash_obj(&blob))
    }

    /// converts relative_paths to absolute paths
    /// checks absolute paths exist and have a base relative to the bit directory
    // can't figure out how to make this take an impl AsRef<Path> and make lifetimes work out
    pub fn normalize<'p>(&self, path: &'p Path) -> BitResult<Cow<'p, Path>> {
        // `self.worktree` should be a canonical, absolute path
        // and path should be relative to it, so we can just join them
        debug_assert!(self.workdir.is_absolute());
        if path.is_relative() {
            let normalized = path::normalize(&self.workdir.join(&path));
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
    pub fn to_relative_path<'p>(&self, path: &'p Path) -> BitResult<&'p Path> {
        // this seems to work just as well as the pathdiff crate
        debug_assert!(path.is_absolute());
        Ok(path.strip_prefix(&self.workdir)?)
    }

    pub(crate) fn relative_path(&self, path: impl AsRef<Path>) -> BitPath {
        self.bitdir.join(path)
    }

    #[cfg(test)]
    pub(crate) fn relative_paths(&self, paths: &[impl AsRef<Path>]) -> BitPath {
        paths.iter().fold(self.bitdir, |base, path| base.join(path))
    }

    pub(crate) fn mk_bitdir(&self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.relative_path(path))
    }

    pub(crate) fn mk_bitfile(&self, path: impl AsRef<Path>) -> io::Result<File> {
        File::create(self.relative_path(path))
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
        &self.ctxt
    }
}

#[cfg(test)]
mod tests;
