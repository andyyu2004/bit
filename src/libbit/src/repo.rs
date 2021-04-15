use crate::error::BitResult;
use crate::hash::BitHash;
use crate::index::BitIndex;
use crate::lockfile::Lockfile;
use crate::obj::{BitId, BitObj, BitObjHeader, BitObjKind};
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::BitPath;
use crate::refs::BitRef;
use crate::serialize::{Deserialize, Serialize};
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

pub struct BitRepo {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub worktree: BitPath,
    pub bitdir: BitPath,
    head_filepath: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
    odb: BitObjDb,
}

impl Debug for BitRepo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<bitrepo>")
    }
}

#[inline]
fn repo_relative_path(repo: &BitRepo, path: impl AsRef<Path>) -> BitPath {
    repo.bitdir.join(path)
}

impl BitRepo {
    fn new(worktree: PathBuf, bitdir: PathBuf, config_filepath: PathBuf) -> Self {
        let worktree = BitPath::intern(worktree);
        let bitdir = BitPath::intern(bitdir);
        let config_filepath = BitPath::intern(config_filepath);
        Self {
            config_filepath,
            worktree,
            bitdir,
            index_filepath: bitdir.join(BIT_INDEX_FILE_PATH),
            head_filepath: bitdir.join(BIT_HEAD_FILE_PATH),
            odb: BitObjDb::new(bitdir.join(BIT_OBJECTS_DIR_PATH)),
        }
    }

    pub fn default_signature(&self) -> BitResult<BitSignature> {
        todo!()
        // BitSignature { name: self.config, email: , time: () }
    }

    /// recursively searches parents starting from the current directory for a git repo
    pub fn find<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(&BitRepo) -> BitResult<R>,
    ) -> BitResult<R> {
        let path = path.as_ref();
        let canonical_path = path.canonicalize().with_context(|| {
            format!("failed to find bit repository in nonexistent path `{}`", path.display())
        })?;
        let repo = Self::find_inner(canonical_path.as_ref())?;
        tls::enter_repo(&repo, f)
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
            None => Err(anyhow!("not a bit repository (or any of the parent directories")),
        }
    }

    #[inline]
    pub fn config_path(&self) -> &Path {
        &self.config_filepath
    }

    #[inline]
    pub fn head_path(&self) -> &Path {
        &self.head_filepath
    }

    #[inline]
    pub fn index_path(&self) -> &Path {
        &self.index_filepath
    }

    pub fn read_head(&self) -> BitResult<Option<BitRef>> {
        Lockfile::with_readonly(self.head_path(), |lockfile| {
            lockfile.file().map(BitRef::deserialize_unbuffered).transpose()
        })
    }

    pub fn write_head(&self, bitref: BitRef) -> BitResult<()> {
        Lockfile::with_mut(self.head_path(), |lockfile| {
            bitref.serialize(lockfile)?;
            Ok(())
        })
    }

    pub fn with_index<R>(&self, f: impl FnOnce(&BitIndex) -> BitResult<R>) -> BitResult<R> {
        Lockfile::with_readonly(self.index_path(), |lockfile| {
            // not actually writing anything here, so we rollback
            // the lockfile is just to check that another process
            // is not currently writing to the index
            f(&BitIndex::from_lockfile(&lockfile)?)
        })
    }

    pub fn with_index_mut<R>(&self, f: impl FnOnce(&mut BitIndex) -> BitResult<R>) -> BitResult<R> {
        Lockfile::with_mut(self.index_path(), |lockfile| {
            let index = &mut BitIndex::from_lockfile(&lockfile)?;
            let r = f(index)?;
            index.serialize(lockfile)?;
            Ok(r)
        })
    }

    fn load(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::load_with_bitdir(path, ".git")
    }

    fn load_with_bitdir(path: impl AsRef<Path>, bitdir: impl AsRef<Path>) -> BitResult<Self> {
        let worktree = path.as_ref().canonicalize()?;
        let bitdir = worktree.join(bitdir);
        assert!(bitdir.exists());
        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);
        let this = Self::new(worktree, bitdir, config_filepath);

        this.with_local_config(|config| {
            let version = config.repositoryformatversion()?.unwrap();
            if version != 0 {
                panic!("Unsupported repositoryformatversion {}", version)
            }
            Ok(())
        })?;

        Ok(this)
    }

    pub fn init(path: impl AsRef<Path>) -> BitResult<Self> {
        let worktree = path.as_ref().canonicalize()?;

        if worktree.is_file() {
            bail!("`{}` is not a directory", worktree.display())
        }

        // `.git` directory not `.bit` as this should be fully compatible with git
        // although, bit will recognize a `.bit` folder if explicitly renamed
        let bitdir = worktree.join(".git");

        if bitdir.exists() {
            // reinitializing doesn't really do anything currently
            println!("reinitializing existing bit directory in `{}`", bitdir.display());
            return Self::load(worktree);
        }

        std::fs::create_dir(&bitdir)?;

        let config_filepath = bitdir.join(BIT_CONFIG_FILE_PATH);

        let this = Self::new(worktree, bitdir, config_filepath);
        this.mk_bitdir("objects")?;
        this.mk_bitdir("branches")?;
        this.mk_bitdir("refs/tags")?;
        this.mk_bitdir("refs/heads")?;

        let mut desc = this.mk_bitfile("description")?;
        writeln!(desc, "Unnamed repository; edit this file 'description' to name the repository.")?;

        let mut head = this.mk_bitfile("HEAD")?;
        writeln!(head, "ref: refs/heads/master")?;

        Ok(this)
    }

    /// todo only works with full hash
    pub fn get_full_object_hash(&self, id: BitId) -> BitResult<BitHash> {
        match id {
            BitId::FullHash(hash) => Ok(hash),
            BitId::PartialHash(_partial) => todo!(),
        }
    }

    /// writes `obj` into the object store returning its full hash
    pub fn write_obj(&self, obj: &impl BitObj) -> BitResult<BitHash> {
        self.odb.write(obj)
    }

    pub fn read_obj(&self, id: impl Into<BitId>) -> BitResult<BitObjKind> {
        self.odb.read(id.into())
    }

    pub fn read_obj_header(&self, id: impl Into<BitId>) -> BitResult<BitObjHeader> {
        self.odb.read_header(id.into())
    }

    /// converts an absolute path into a path relative to the workdir of the repository
    pub(crate) fn to_relative_path(&self, path: impl AsRef<Path>) -> BitResult<BitPath> {
        let path = path.as_ref();
        assert!(path.is_absolute());
        Ok(BitPath::intern(path.strip_prefix(&self.worktree)?))
        // pathdiff::diff_paths(path, &self.worktree)
        // .map(BitPath::intern)
        // .ok_or_else(|| anyhow!("failed to diffpaths to get relative path"))
    }

    pub(crate) fn relative_path(&self, path: impl AsRef<Path>) -> BitPath {
        repo_relative_path(self, path)
    }

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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::cmd::BitHashObjectOpts;
    use crate::obj;

    #[test]
    fn repo_relative_paths() -> BitResult<()> {
        let basedir = tempfile::tempdir()?;
        let repo = BitRepo::init(&basedir)?;
        let joined = repo.relative_paths(&["path", "to", "dir"]);
        assert_eq!(joined, format!("{}/.git/path/to/dir", basedir.path().display()));
        Ok(())
    }

    #[test]
    fn init_on_file() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let filepath = dir.path().join("test");
        File::create(&filepath)?;
        let err = BitRepo::init(filepath).unwrap_err();
        assert!(err.to_string().contains("not a directory"));
        Ok(())
    }

    fn prop_bit_hash_object_cat_file_are_inverses_blob(bytes: Vec<u8>) -> BitResult<()> {
        let basedir = tempfile::tempdir()?;
        let repo = BitRepo::init(basedir.path())?;

        let file_path = basedir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        file.write_all(&bytes)?;

        let hash = repo.bit_hash_object(BitHashObjectOpts {
            path: file_path,
            do_write: true,
            objtype: obj::BitObjType::Blob,
        })?;

        assert!(
            repo.relative_paths(&["objects", &hex::encode(&hash[0..1]), &hex::encode(&hash[1..])])
                .exists()
        );

        // this doesn't call `bit_cat_file` directly but this function is
        // basically all that it does internally
        let blob = repo.read_obj(hash)?.into_blob();

        assert_eq!(blob.bytes, bytes);
        Ok(())
    }

    #[test]
    fn test_bit_hash_object_cat_file_are_inverses_blob() -> BitResult<()> {
        prop_bit_hash_object_cat_file_are_inverses_blob(b"hello".to_vec())
    }

    #[quickcheck]
    fn bit_hash_object_cat_file_are_inverses_blob(bytes: Vec<u8>) -> BitResult<()> {
        prop_bit_hash_object_cat_file_are_inverses_blob(bytes)
    }
}
