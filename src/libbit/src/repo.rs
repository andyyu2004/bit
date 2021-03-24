use crate::error::BitResult;
use crate::hash::{self, BitHash};
use crate::index::BitIndex;
use crate::obj::{self, BitObj, BitObjId, BitObjKind, BitObjType};
use crate::signature::BitSignature;
use crate::tls;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::lazy::OnceCell;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const BIT_INDEX_FILE_PATH: &str = "index";
pub const BIT_CONFIG_FILE_PATH: &str = "config";

pub struct BitRepo {
    pub worktree: PathBuf,
    pub bitdir: PathBuf,
    config_filepath: PathBuf,
    index_filepath: PathBuf,
    index: OnceCell<RefCell<BitIndex>>,
}

impl Debug for BitRepo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<bitrepo>")
    }
}

#[inline]
fn repo_relative_path(repo: &BitRepo, path: impl AsRef<Path>) -> PathBuf {
    repo.bitdir.join(path)
}

impl BitRepo {
    fn new(worktree: PathBuf, bitdir: PathBuf, config_filepath: PathBuf) -> Self {
        Self {
            index_filepath: bitdir.join(BIT_INDEX_FILE_PATH),
            index: OnceCell::new(),
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
            return Err(anyhow!("`{}` is not a directory", worktree.display()))?;
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
    pub fn get_full_object_hash(&self, id: BitObjId) -> BitResult<BitHash> {
        match id {
            BitObjId::FullHash(hash) => Ok(hash),
            BitObjId::PartialHash(_partial) => todo!(),
        }
    }

    /// writes `obj` into the object store returning its full hash
    pub fn write_obj(&self, obj: &impl BitObj) -> BitResult<BitHash> {
        let bytes = obj::serialize_obj_with_headers(obj)?;
        let hash = hash::hash_bytes(bytes.as_slice());
        let (directory, file_path) = hash.split();
        self.with_bitfile(&["objects", &directory, &file_path], |file| {
            let mut encoder = ZlibEncoder::new(file, Compression::default());
            encoder.write_all(&bytes)?;
            Ok(hash)
        })
    }

    pub fn obj_filepath_from_hash(&self, hash: &BitHash) -> PathBuf {
        let (dir, file) = hash.split();
        self.relative_paths(&["objects", &dir, &file])
    }

    pub fn obj_stream_from_hash(&self, hash: &BitHash) -> BitResult<impl Read> {
        let obj_path = self.obj_filepath_from_hash(hash);
        let file = File::open(obj_path)?;
        Ok(ZlibDecoder::new(file))
    }

    pub fn obj_stream_from_id(&self, id: BitObjId) -> BitResult<impl Read> {
        let hash = self.get_full_object_hash(id)?;
        let obj_path = self.obj_filepath_from_hash(&hash);
        let file = File::open(obj_path)?;
        Ok(ZlibDecoder::new(file))
    }

    pub fn read_obj_as<T: BitObj>(&self, _id: impl Into<BitObjId>) -> BitResult<T> {
        todo!()
        // self.read_obj_from_id(id.into())
    }

    pub fn read_obj(&self, id: impl Into<BitObjId>) -> BitResult<BitObjKind> {
        self.read_obj_from_id(id.into())
    }

    pub fn read_obj_from_id(&self, id: BitObjId) -> BitResult<BitObjKind> {
        let hash = self.get_full_object_hash(id)?;
        let stream = self.obj_stream_from_hash(&hash)?;
        obj::read_obj(stream)
    }

    pub fn read_obj_type_from_hash(&self, hash: &BitHash) -> BitResult<BitObjType> {
        let stream = self.obj_stream_from_hash(hash)?;
        obj::read_obj_type(stream)
    }

    pub fn read_obj_type_from_id(&self, id: BitObjId) -> BitResult<BitObjType> {
        let stream = self.obj_stream_from_id(id)?;
        obj::read_obj_type(stream)
    }

    pub fn read_obj_size_from_id(&self, id: BitObjId) -> BitResult<usize> {
        let stream = self.obj_stream_from_id(id)?;
        obj::read_obj_size_from_start(stream)
    }

    pub fn read_obj_from_hash(&self, hash: &BitHash) -> BitResult<BitObjKind> {
        let stream = self.obj_stream_from_hash(&hash)?;
        obj::read_obj(stream)
    }

    pub(crate) fn relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        repo_relative_path(self, path)
    }

    pub(crate) fn relative_paths(&self, paths: &[impl AsRef<Path>]) -> PathBuf {
        paths.iter().fold(self.bitdir.to_path_buf(), |base, path| base.join(path))
    }

    pub(crate) fn mk_bitdir(&self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.relative_path(path))
    }

    pub(crate) fn with_bitfile<R>(
        &self,
        paths: &[impl AsRef<Path>],
        f: impl FnOnce(&mut File) -> BitResult<R>,
    ) -> BitResult<R> {
        let path = self.relative_paths(paths);
        // the parent path must exist as this file is definitely not in the root directory
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent)?;
        // create a temporary file and perform all mutations there to avoid any
        // filesystem related race conditions
        // the rename/mv operation is atomic
        let mut tmp_file = NamedTempFile::new_in(parent)?;
        let ret = f(tmp_file.as_file_mut())?;
        let mut permissions = tmp_file.as_file().metadata()?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(tmp_file.path(), permissions)?;

        if path.exists() {
            let mut permissions = std::fs::metadata(&path)?.permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions)?;
        }

        std::fs::rename(tmp_file.path(), path)?;

        Ok(ret)
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
        assert_eq!(joined, PathBuf::from(format!("{}/.git/path/to/dir", basedir.path().display())));
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
        let blob = repo.read_obj_from_hash(&hash)?.as_blob();

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
