use crate::error::BitResult;
use crate::serialize::Deserialize;
use crate::serialize::Serialize;
use anyhow::Context;
use bitflags::bitflags;
use std::cell::Cell;
use std::fs::File;
use std::io::BufReader;
use std::io::{self, prelude::*};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

const LOCK_FILE_EXT: &str = "lock";

bitflags! {
    pub struct LockfileFlags: u8 {
        const SET_READONLY = 1;
    }
}

// TODO this design is getting a bit messy now with committed and rolled_back flags and what not with arbitrary dependencies between them

#[derive(Debug)]
pub struct Lockfile {
    // the file that this lockfile is guarding
    // None if it does not exist
    file: Option<File>,
    // the lockfile itself
    lockfile: File,
    flags: LockfileFlags,
    path: PathBuf,
    lockfile_path: PathBuf,
    committed: Cell<bool>,
    rolled_back: Cell<bool>,
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
    //
    // consumers of this api should never have access to the lockfile
    // this will create the file if it doesn't exist
    // directly, instead they should use the `with_` apis
    fn open(path: impl AsRef<Path>, flags: LockfileFlags) -> BitResult<Self> {
        let path = path.as_ref();
        assert!(!path.exists() || path.is_file(), "cannot create lock on symlinks or directories");
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
            flags,
            lockfile_path,
            path: path.to_path_buf(),
            committed: Cell::new(false),
            rolled_back: Cell::new(false),
        })
    }

    // should never have mutable access to `self.file`
    // as any writes should be done to the lockfile only
    pub fn file(&self) -> Option<&File> {
        self.file.as_ref()
    }

    pub fn with_readonly<R>(
        path: impl AsRef<Path>,
        flags: LockfileFlags,
        f: impl FnOnce(&Self) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::open(path, flags)?.with_readonly_inner(f)
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
        flags: LockfileFlags,
        f: impl FnOnce(&mut Self) -> BitResult<R>,
    ) -> BitResult<R> {
        Self::open(path, flags)?.with_mut_inner(f)
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
        // ignore commit after a rollback
        if self.rolled_back.get() {
            return Ok(());
        }
        let set_readonly = self.flags.contains(LockfileFlags::SET_READONLY);
        // we only do this branch if we expect it to be readonly
        // if its when this flag is false then that is unexpected and we should get an error
        if set_readonly && self.path.exists() {
            let mut permissions = self.path.metadata()?.permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&self.path, permissions)?;
        }

        std::fs::rename(&self.lockfile_path, &self.path)?;
        self.committed.set(true);

        if set_readonly {
            let mut permissions = self.path.metadata()?.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&self.path, permissions)?;
        }

        Ok(())
    }

    fn cleanup(&self) -> BitResult<()> {
        std::fs::remove_file(&self.lockfile_path).with_context(|| {
            format!("failed to remove lockfile `{}`", self.lockfile_path.display())
        })
    }

    pub fn rollback(&self) {
        // don't do anything until the drop impl
        self.rolled_back.set(true);
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        // can't be both rolled_back and committed
        assert!(!self.rolled_back.get() || !self.committed.get());
        // if either explicitly rolled back, or not explicitly committed, then rollback
        if self.rolled_back.get() || !self.committed.get() {
            self.cleanup().unwrap();
        }
    }
}

/// the default is `commit`, `rollback` must be explicit
/// data must not have interior mutability otherwise changes may be ignored
pub struct Filelock<T: Serialize> {
    data: T,
    lockfile: Lockfile,
    has_changes: bool,
    rolled_back: bool,
}

impl<T: Serialize + Deserialize + Default> Filelock<T> {
    pub fn lock_with_flags(path: impl AsRef<Path>, flags: LockfileFlags) -> BitResult<Self> {
        let mut lockfile = Lockfile::open(path, flags)?;
        let data = match &mut lockfile.file {
            Some(file) => T::deserialize(&mut BufReader::new(file))?,
            None => T::default(),
        };
        Ok(Filelock { lockfile, data, has_changes: false, rolled_back: false })
    }

    pub fn lock(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::lock_with_flags(path, LockfileFlags::empty())
    }
}

impl<T: Serialize> Filelock<T> {
    pub fn rollback(&mut self) {
        self.rolled_back = true;
        self.lockfile.rollback();
    }
}

impl<T: Serialize> Drop for Filelock<T> {
    fn drop(&mut self) {
        if self.rolled_back || !self.has_changes {
            // the lockfile drop impl will do the necessary cleanup
            return;
        }
        // otherwise, write the data into the lockfile and commit
        self.data.serialize(&mut self.lockfile).expect("failed to write data (in Filelock)");
        self.lockfile.commit().expect("failed to commit lockfile (in Filelock)");
    }
}

impl<T: Serialize> Deref for Filelock<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Serialize> DerefMut for Filelock<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // conservatively assume any mutable access results in a change
        self.has_changes = true;
        &mut self.data
    }
}

#[cfg(test)]
mod tests;
