use crate::error::BitResult;
use crate::hash;
use crate::index::BitIndex;
use crate::lockfile::Lockfile;
use crate::obj::{BitId, BitObj, BitObjHeader, BitObjKind, Blob, Oid, PartialOid, Tree, Treeish};
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::{self, BitPath};
use crate::refs::RefUpdateCause;
use crate::refs::{BitRef, BitRefDb, BitRefDbBackend, SymbolicRef};
use crate::serialize::Serialize;
use crate::signature::BitSignature;
use crate::tls;
use anyhow::Context;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const BIT_INDEX_FILE_PATH: &str = "index";
pub const BIT_HEAD_FILE_PATH: &str = "HEAD";
pub const BIT_CONFIG_FILE_PATH: &str = "config";
pub const BIT_OBJECTS_DIR_PATH: &str = "objects";

#[derive(Copy, Clone)]
pub struct BitRepo {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub workdir: BitPath,
    pub bitdir: BitPath,
    head_filepath: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
}

trait Repo {
    type Odb: BitObjDbBackend;
    type RefDb: BitRefDbBackend;

    fn odb(&self) -> Self::Odb;
    fn refdb(&self) -> Self::RefDb;
}

impl Repo for BitRepo {
    type Odb = BitObjDb;
    type RefDb = BitRefDb;

    fn odb(&self) -> Self::Odb {
        // TODO shouldn't have to recreate this everytime
        // TODO should be able to just return a reference somehow
        BitObjDb::new(self.bitdir.join(BIT_OBJECTS_DIR_PATH)).expect("todo error handling")
    }

    fn refdb(&self) -> Self::RefDb {
        BitRefDb::new(*self)
    }
}

impl BitRepo {
    pub fn default_signature(&self) -> BitResult<BitSignature> {
        todo!()
        // BitSignature { name: self.config, email: , time: () }
    }

    /// initialize a repository and use it in the closure
    pub fn init_load<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(&Self) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::init(&path)?;
        let repo = Self::load(&path)?;
        tls::enter_repo(&repo, f)
    }

    /// recursively searches parents starting from the current directory for a git repo
    pub fn find<R>(path: impl AsRef<Path>, f: impl FnOnce(&Self) -> BitResult<R>) -> BitResult<R> {
        let path = path.as_ref();
        let canonical_path = path.canonicalize().with_context(|| {
            format!("failed to find bit repository in nonexistent path `{}`", path.display())
        })?;
        let repo = Self::find_inner(canonical_path.as_ref())?;
        tls::enter_repo(&repo, f)
    }

    fn new(workdir: PathBuf, bitdir: PathBuf, config_filepath: PathBuf) -> BitResult<Self> {
        let workdir = BitPath::intern(workdir);
        let bitdir = BitPath::intern(bitdir);
        let config_filepath = BitPath::intern(config_filepath);
        Ok(Self {
            config_filepath,
            workdir,
            bitdir,
            index_filepath: bitdir.join(BIT_INDEX_FILE_PATH),
            head_filepath: bitdir.join(BIT_HEAD_FILE_PATH),
        })
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

    #[inline]
    pub fn config_path(&self) -> BitPath {
        self.config_filepath
    }

    #[inline]
    pub fn head_path(&self) -> BitPath {
        self.head_filepath
    }

    #[inline]
    pub fn head_ref(&self) -> SymbolicRef {
        SymbolicRef::new(self.head_filepath)
    }

    #[inline]
    pub fn index_path(&self) -> BitPath {
        self.index_filepath
    }

    pub fn with_index<R>(&self, f: impl FnOnce(&BitIndex<'_>) -> BitResult<R>) -> BitResult<R> {
        Lockfile::with_readonly(self.index_path(), |lockfile| {
            // not actually writing anything here, so we rollback
            // the lockfile is just to check that another process
            // is not currently writing to the index
            f(&BitIndex::from_lockfile(self, &lockfile)?)
        })
    }

    pub fn with_index_mut<R>(
        &self,
        f: impl FnOnce(&mut BitIndex<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        Lockfile::with_mut(self.index_path(), |lockfile| {
            let index = &mut BitIndex::from_lockfile(self, &lockfile)?;
            let r = f(index)?;
            index.serialize(lockfile)?;
            Ok(r)
        })
    }

    fn load(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::load_with_bitdir(path, ".git")
    }

    fn load_with_bitdir(path: impl AsRef<Path>, bitdir: impl AsRef<Path>) -> BitResult<Self> {
        let worktree = path
            .as_ref()
            .canonicalize()
            .with_context(|| anyhow!("failed to load bit in non-existent directory"))?;
        let bitdir = worktree.join(bitdir);
        assert!(bitdir.exists());
        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);
        let this = Self::new(worktree, bitdir, config_filepath)?;

        this.with_local_config(|config| {
            let version = config
                .repositoryformatversion()?
                .expect("`repositoryformatversion` missing in configuration");
            if version != 0 {
                panic!("Unsupported repositoryformatversion {}", version)
            }
            Ok(())
        })?;

        Ok(this)
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

        let this = Self::new(workdir, bitdir, config_filepath)?;
        this.mk_bitdir("objects")?;
        this.mk_bitdir("refs/tags")?;
        this.mk_bitdir("refs/heads")?;

        let mut desc = this.mk_bitfile("description")?;
        writeln!(desc, "Unnamed repository; edit this file 'description' to name the repository.")?;

        let mut head = this.mk_bitfile("HEAD")?;
        writeln!(head, "ref: refs/heads/master")?;

        this.with_local_config(|config| {
            config.set("core", "repositoryformatversion", 0)?;
            config.set("core", "bare", false)?;
            config.set("core", "filemode", true)?;
            Ok(())
        })?;

        println!("initialized empty bit repository in `{}`", this.workdir.display());
        Ok(())
    }

    /// todo only works with full hash
    pub fn get_full_object_hash(&self, id: BitId) -> BitResult<Oid> {
        match id {
            BitId::Full(hash) => Ok(hash),
            BitId::Partial(_partial) => todo!(),
        }
    }

    /// the tree belonging to the `HEAD` reference, or an empty tree if either
    /// HEAD does not exist, or is not fully resolvable
    pub fn head_tree(&self) -> BitResult<Tree> {
        let oid = match self.resolve_head()? {
            BitRef::Direct(oid) => oid,
            _ => return Ok(Tree::default()),
        };
        let commit = self.read_obj(oid)?.into_commit();
        Ok(self.read_obj(commit.tree())?.into_tree()?)
    }

    /// returns the resolved hash of the HEAD symref
    pub fn resolve_head(&self) -> BitResult<BitRef> {
        let head = self.read_head()?;
        self.resolve_ref(head)
    }

    pub fn partially_resolve_head(&self) -> BitResult<BitRef> {
        self.read_head()?.partially_resolve(self)
    }

    /// reads the contents of `HEAD`
    /// e.g. if `HEAD` -> `ref: refs/heads/master`
    /// then `BitRef::Symbolic(SymbolicRef("refs/heads/master"))` is returned`
    pub fn read_head(&self) -> BitResult<BitRef> {
        self.refdb().read(self.head_ref())
    }

    pub fn update_head(&self, bitref: impl Into<BitRef>, cause: RefUpdateCause) -> BitResult<()> {
        self.update_ref(self.head_ref(), bitref.into(), cause)
    }

    pub fn create_branch(&self, sym: SymbolicRef, from: SymbolicRef) -> BitResult<()> {
        // we fully resolve the reference to an oid and write that into the new branch file
        let resolved = from.fully_resolve(self)?;
        self.refdb().create_branch(sym, resolved.into())
    }

    pub fn update_ref(
        &self,
        sym: SymbolicRef,
        to: impl Into<BitRef>,
        cause: RefUpdateCause,
    ) -> BitResult<()> {
        self.refdb().update(sym, to.into(), cause)
    }

    /// writes `obj` into the object store returning its full hash
    pub fn write_obj(&self, obj: &dyn BitObj) -> BitResult<Oid> {
        self.odb().write(obj)
    }

    pub fn read_obj(&self, id: impl Into<BitId>) -> BitResult<BitObjKind> {
        self.odb().read(id.into())
    }

    pub fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid> {
        self.odb().expand_prefix(prefix)
    }

    pub fn obj_exists(&self, id: impl Into<BitId>) -> BitResult<bool> {
        self.odb().exists(id.into())
    }

    pub fn read_obj_header(&self, id: impl Into<BitId>) -> BitResult<BitObjHeader> {
        self.odb().read_header(id.into())
    }

    pub fn hash_blob(&self, path: BitPath) -> BitResult<Oid> {
        let path = self.normalize(path)?;
        let bytes = path.read_to_vec()?;
        let blob = Blob::new(bytes);
        hash::hash_obj(&blob)
    }

    /// converts relative_paths to absolute paths
    /// checks absolute paths exist and have a base relative to the bit directory
    pub fn normalize(&self, path: impl AsRef<Path>) -> BitResult<BitPath> {
        // `self.worktree` should be a canonical, absolute path
        // and path should be relative to it, so we can just join them
        debug_assert!(self.workdir.is_absolute());
        let path = path.as_ref();
        if path.is_relative() {
            let normalized = path::normalize(&self.workdir.join(&path));
            ensure!(
                normalized.symlink_metadata().is_ok(),
                "normalized path `{}` does not exist",
                normalized.display()
            );
            Ok(BitPath::intern(normalized))
        } else {
            assert!(
                path.starts_with(&self.workdir),
                "absolute path `{}` is not under current bit directory `{}`",
                path.display(),
                self.workdir
            );
            Ok(BitPath::intern(path))
        }
    }

    /// converts an absolute path into a path relative to the workdir of the repository
    pub fn to_relative_path(&self, path: impl AsRef<Path>) -> BitResult<BitPath> {
        // this seems to work just as well as the pathdiff crate
        let path = path.as_ref();
        assert!(path.is_absolute());
        Ok(BitPath::intern(path.strip_prefix(&self.workdir)?))
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

impl Debug for BitRepo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitRepo")
            .field("worktree", &self.workdir)
            .field("bitdir", &self.bitdir)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
