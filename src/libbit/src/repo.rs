use crate::error::{BitError, BitResult};
use crate::hash::{self, BitHash};
use crate::obj::{self, BitObj, BitObjId, BitObjKind};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use ini::Ini;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct BitRepo {
    worktree: PathBuf,
    bitdir: PathBuf,
    config: Ini,
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
    pub fn find_in_current_dir() -> BitResult<Self> {
        Self::find(std::env::current_dir()?)
    }

    /// recursively searches parents starting from the current directory for a git repo
    pub fn find(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::find_inner(path.as_ref())
    }

    fn find_inner(path: &Path) -> BitResult<Self> {
        if path.join(".git").exists() {
            return Self::load(path);
        }

        match path.parent() {
            Some(parent) => Self::find_inner(parent),
            None => Err(BitError::BitDirectoryNotFound),
        }
    }

    fn load(path: impl AsRef<Path>) -> BitResult<Self> {
        let worktree = path.as_ref().canonicalize()?;
        let bitdir = worktree.join(".git");
        assert!(bitdir.exists());
        let config = Ini::load_from_file(bitdir.join("config"))?;
        let version = &config["core"]["repositoryformatversion"];
        if version.parse::<i32>().unwrap() != 0 {
            panic!("Unsupported repositoryformatversion {}", version)
        }
        Ok(Self { worktree, bitdir, config })
    }

    pub fn init(path: impl AsRef<Path>) -> BitResult<Self> {
        let path = path.as_ref();

        if !path.exists() {
            std::fs::create_dir(path)?
        }

        let worktree = path.canonicalize()?;

        if worktree.is_file() {
            return Err(BitError::NotDirectory(worktree))?;
        }

        // `.git` directory not `.bit` as this should be compatible with git
        let bitdir = worktree.join(".git");

        if bitdir.exists() {
            // reinitializing doesn't really do anything currently
            println!("reinitializing existing bit directory in `{}`", bitdir.display());
            return Self::load(worktree);
        }

        std::fs::create_dir(&bitdir)?;

        let config = Self::default_config();
        config.write_to_file(bitdir.join("config"))?;

        let this = Self { worktree, bitdir, config };
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

    fn default_config() -> Ini {
        let mut ini = Ini::default();
        ini.with_section(Some("core"))
            .set("repositoryformatversion", "0")
            .set("filemode", "false")
            .set("bare", "false");
        ini
    }

    /// todo only works with full hash
    pub fn find_obj(&self, id: BitObjId) -> BitResult<BitHash> {
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
        let file = self.mk_nested_bitfile(&["objects", &directory, &file_path])?;
        let mut encoder = ZlibEncoder::new(file, Compression::default());
        encoder.write_all(&bytes)?;
        Ok(hash)
    }

    pub fn read_obj_from_hash(&self, hash: &BitHash) -> BitResult<BitObjKind> {
        let (dir, file) = hash.split();
        let obj_path = self.relative_paths(&["objects", &dir, &file]);
        let z = ZlibDecoder::new(File::open(obj_path)?);
        obj::read_obj(z)
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

    pub(crate) fn mk_nested_bitfile(&self, paths: &[impl AsRef<Path>]) -> io::Result<File> {
        let path = self.relative_paths(paths);
        path.parent().map(|parent| fs::create_dir_all(parent));
        File::create(path)
    }

    pub(crate) fn mk_bitfile(&self, path: impl AsRef<Path>) -> io::Result<File> {
        File::create(self.relative_path(path))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::cli::{BitCatFileOpts, BitHashObjectOpts};
    use crate::obj::BitObjType;

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
        match BitRepo::init(filepath).unwrap_err() {
            BitError::NotDirectory(..) => {}
            _ => panic!(),
        }
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
            write: true,
            objtype: obj::BitObjType::Blob,
        })?;

        assert!(
            repo.relative_paths(&["objects", &hex::encode(&hash[0..1]), &hex::encode(&hash[1..])])
                .exists()
        );

        let blob = repo
            .bit_cat_file(BitCatFileOpts {
                id: BitObjId::from_str(&hex::encode(hash)).unwrap(),
                objtype: BitObjType::Blob,
            })?
            .as_blob();

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
