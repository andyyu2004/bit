use crate::error::BitResult;
use anyhow::Context;
use std::cell::Cell;
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};

const LOCK_FILE_EXT: &str = "lock";

#[derive(Debug)]
pub struct Lockfile {
    // the file that this lockfile is guarding
    // None if it does not exist
    file: Option<File>,
    // the lockfile itself
    lockfile: File,
    path: PathBuf,
    lockfile_path: PathBuf,
    committed: Cell<bool>,
}

impl Write for Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.lockfile.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.lockfile.flush()
    }
}

impl Lockfile {
    /// accepts the path to the file to be locked
    /// this function will create a lockfile with an extension `<path>.lock`
    /// consumers of this api should never have access to the lockfile
    /// directly, instead they should use the `with_` apis
    fn open(path: impl AsRef<Path>) -> BitResult<Self> {
        let path = path.as_ref();
        let lockfile_path = path.with_extension(LOCK_FILE_EXT);
        path.parent().map(std::fs::create_dir_all).transpose()?;
        // read comments on `.create_new()` for more info
        let lockfile =
            File::with_options().create_new(true).write(true).open(&lockfile_path).or_else(
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

        let file = path.exists().then(|| File::open(path)).transpose()?;

        Ok(Self {
            file,
            lockfile,
            lockfile_path,
            path: path.to_path_buf(),
            committed: Cell::new(false),
        })
    }

    // should never have mutable access to `self.file`
    // as any writes should be done to the lockfile only
    pub fn file(&self) -> Option<&File> {
        self.file.as_ref()
    }

    pub fn with_readonly<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(&Self) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::open(path)?.with_readonly_inner(f)
    }

    /// run's a function under the lock without having write access to the lock
    /// will never commit anything
    fn with_readonly_inner<R>(&self, f: impl FnOnce(&Self) -> BitResult<R>) -> BitResult<R> {
        let r = f(self);
        self.rollback();
        r
    }

    pub fn with_mut<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(&mut Self) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::open(path)?.with_mut_inner(f)
    }

    /// runs a function under the lock having mutable access to the underlying file
    /// if the closure returns an `Err` then the transaction is rolled back, otherwise it is
    /// committed to disk
    fn with_mut_inner<R>(mut self, f: impl FnOnce(&mut Self) -> BitResult<R>) -> BitResult<R> {
        match f(&mut self) {
            Ok(r) => {
                self.commit().with_context(|| anyhow!(
                        "failed to write lockfile to `{}`;  the updated contents are stored in `{}`; please remove this file when done",
                        self.path.display(),
                        self.lockfile_path.display()
                    )
                )?;
                Ok(r)
            }
            Err(err) => {
                self.rollback();
                Err(err)
            }
        }
    }

    /// commits this file by renaming it to the target file
    /// replaces the old file if it exists
    /// commits on drop unless rollback was called
    fn commit(&self) -> io::Result<()> {
        if self.path.exists() {
            let mut permissions = self.path.metadata()?.permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&self.path, permissions)?;
        }

        std::fs::rename(&self.lockfile_path, &self.path)?;
        self.committed.set(true);

        let mut permissions = self.path.metadata()?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&self.path, permissions)
    }

    fn cleanup(&self) -> BitResult<()> {
        std::fs::remove_file(&self.lockfile_path).with_context(|| {
            format!("failed to remove lockfile `{}`", self.lockfile_path.display())
        })
    }

    fn rollback(&self) {
        // does rollback actually have to anything that the drop impl doesn't do?
        // just exists for semantic purposes for now
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        if self.committed.get() {
            // if committed then the file has been renamed and there is nothing to cleanup
            return;
        }
        self.cleanup().unwrap();
    }
}
