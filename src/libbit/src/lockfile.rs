use crate::error::BitResult;
use anyhow::Context;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const LOCK_FILE_EXT: &str = "lock";

pub struct Lockfile {
    file: File,
    path: PathBuf,
    lockfile_path: PathBuf,
    aborted: bool,
}

impl Lockfile {
    pub fn exists(path: impl AsRef<Path>) -> BitResult<bool> {
        match File::with_options().create_new(true).write(true).open(&path) {
            Ok(_) => Ok(false),
            Err(err) => match err.kind() {
                io::ErrorKind::AlreadyExists => Ok(true),
                _ =>
                    return Err(err).with_context(|| {
                        format!("failed to check if file exists at `{}`", path.as_ref().display())
                    }),
            },
        }
    }

    pub fn new(path: impl AsRef<Path>) -> BitResult<Self> {
        let path = path.as_ref();
        let lockfile_path = path.with_extension(LOCK_FILE_EXT);
        // read comments on `.create_new()` for more info
        let file =
            File::with_options().create_new(true).write(true).open(path).or_else(
                |err| match err.kind() {
                    io::ErrorKind::AlreadyExists => Err(err).with_context(|| {
                        format!(
                            "failed to lock file `{}` (`{}` already exists)",
                            path.display(),
                            lockfile_path.display()
                        )
                    }),
                    _ => Err(err)
                        .with_context(|| format!("failed to create file `{}`", path.display())),
                },
            )?;

        Ok(Self { file, path: path.to_path_buf(), lockfile_path, aborted: false })
    }

    pub fn write(&mut self, contents: &[u8]) -> io::Result<()> {
        self.file.write_all(contents)
    }

    /// commits this file by renaming it to the target file
    /// commits on drop unless rollback was called
    fn commit(&self) -> io::Result<()> {
        std::fs::rename(&self.path, &self.lockfile_path)
    }

    fn cleanup(&self) -> BitResult<()> {
        std::fs::remove_file(&self.lockfile_path).with_context(|| {
            format!("failed to remove lockfile `{}`", self.lockfile_path.display())
        })
    }

    fn rollback(&mut self) -> BitResult<()> {
        self.aborted = true;
        self.cleanup()
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        if self.aborted {
            return;
        }

        self.commit().unwrap_or_else(|err| {
            panic!(
                "failed to write lockfile to `{}`: {}; the updated contents are stored in `{}`; please remove this file when done",
                self.path.display(),
                err,
                self.lockfile_path.display()
            )
        });

        self.cleanup().unwrap()
    }
}
