use crate::error::BitResult;
use anyhow::Context;
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};

const LOCK_FILE_EXT: &str = "lock";

pub struct Lockfile {
    file: File,
    path: PathBuf,
    lockfile_path: PathBuf,
    aborted: bool,
}

impl Write for Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Lockfile {
    pub fn new(path: impl AsRef<Path>) -> BitResult<Self> {
        let path = path.as_ref();
        let lockfile_path = path.with_extension(LOCK_FILE_EXT);
        path.parent().map(|parent| std::fs::create_dir_all(parent)).transpose()?;
        // read comments on `.create_new()` for more info
        let file = File::with_options().create_new(true).write(true).open(&lockfile_path).or_else(
            |err| match err.kind() {
                io::ErrorKind::AlreadyExists => Err(err).with_context(|| {
                    format!(
                        "failed to lock file `{}` (`{}` already exists)",
                        path.display(),
                        lockfile_path.display()
                    )
                }),
                _ =>
                    Err(err).with_context(|| format!("failed to create file `{}`", path.display())),
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
        if self.path.exists() {
            let mut permissions = self.path.metadata()?.permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&self.path, permissions)?;
        }

        std::fs::rename(&self.lockfile_path, &self.path)?;

        let mut permissions = self.path.metadata()?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&self.path, permissions)
    }

    fn cleanup(&self) -> BitResult<()> {
        std::fs::remove_file(&self.lockfile_path).with_context(|| {
            format!("failed to remove lockfile `{}`", self.lockfile_path.display())
        })
    }

    pub fn rollback(&mut self) -> BitResult<()> {
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
    }
}
