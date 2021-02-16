use crate::error::BitResult;
use crate::BitError;
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
    pub fn load(path: impl AsRef<Path>) -> BitResult<Self> {
        let path = path.as_ref();
        let worktree = path.canonicalize()?;
        let bitdir = path.join(".git");
        if !bitdir.exists() {
            todo!("`{}` is not a bit repository", bitdir.display());
        }
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

        if !worktree.read_dir()?.next().is_none() {
            return Err(BitError::NonEmptyDirectory(worktree))?;
        }

        // `.git` directory not `.bit` as this should be fully compatible with git
        let bitdir = worktree.join(".git");
        debug_assert!(!bitdir.exists());
        std::fs::create_dir(&bitdir)?;

        let config = Self::default_config();
        Self::default_config().write_to_file(bitdir.join("config"))?;

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

    fn relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        repo_relative_path(self, path)
    }

    fn mk_bitdir(&self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.relative_path(path))
    }

    fn mk_bitfile(&self, path: impl AsRef<Path>) -> io::Result<File> {
        File::create(self.relative_path(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn init_on_non_empty_dir() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let dirpath = dir.path().join(".git");
        File::create(&dirpath)?;
        match BitRepo::init(dir).unwrap_err() {
            BitError::NonEmptyDirectory(..) => {}
            _ => panic!(),
        }
        Ok(())
    }
}
