use crate::error::BitResult;
use crate::hash::{self, BitHash};
use crate::index::BitIndex;
use crate::obj::{self, BitId, BitObj, BitObjHeader, BitObjKind, BitObjType};
use crate::odb::{BitObjDb, BitObjDbBackend};
use crate::path::BitPath;
use crate::signature::BitSignature;
use crate::tls;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::lazy::OnceCell;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const BIT_INDEX_FILE_PATH: &str = "index";
pub const BIT_OBJECTS_FILE_PATH: &str = "objects";
pub const BIT_CONFIG_FILE_PATH: &str = "config";

pub struct BitRepo {
    // ok to make this public as there is only ever
    // shared (immutable) access to this struct
    pub worktree: BitPath,
    pub bitdir: BitPath,
    config_filepath: BitPath,
    index_filepath: BitPath,
    index: OnceCell<RefCell<BitIndex>>,
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
            index_filepath: bitdir.join(BIT_INDEX_FILE_PATH),
            index: OnceCell::new(),
            odb: BitObjDb::new(bitdir.join(BIT_OBJECTS_FILE_PATH)),
            config_filepath,
            worktree,
            bitdir,
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
        let canonical_path = path.as_ref().canonicalize()?;
        let repo = Self::find_inner(canonical_path.as_ref())?;
        tls::with_repo(&repo, f)
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

    pub fn config_path(&self) -> &Path {
        &self.config_filepath
    }

    pub fn index_path(&self) -> &Path {
        &self.index_filepath
    }

    fn index_file(&self) -> BitResult<File> {
        let file = File::open(self.index_path());
        // assume an error means the file doesn't exist
        // not ideal but probably true is almost every case
        if file.is_err() {
            assert!(!self.index_path().exists());
        }
        Ok(file?)
    }

    fn get_index(&self) -> &RefCell<BitIndex> {
        let mk_index = || {
            let index = self.index_file().and_then(BitIndex::deserialize).unwrap_or_default();
            RefCell::new(index)
        };
        self.index.get_or_init(mk_index)
    }

    pub fn with_index<R>(&self, f: impl FnOnce(&BitIndex) -> R) -> R {
        f(&self.get_index().borrow())
    }

    pub fn with_index_mut<R>(&self, f: impl FnOnce(&mut BitIndex) -> R) -> R {
        f(&mut self.get_index().borrow_mut())
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

        return Ok(this);
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

impl Drop for BitRepo {
    fn drop(&mut self) {
        if let Some(_index) = self.index.get() {
            // TODO write index?
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::cmd::BitHashObjectOpts;

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
        let blob = repo.read_obj(hash)?.as_blob();

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
