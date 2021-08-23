use crate::error::BitResult;
use crate::serialize::Deserialize;
use crate::serialize::Serialize;
use anyhow::Context;
use bitflags::bitflags;
use std::fs::File;
use std::io::{self, prelude::*};
use std::io::{BufReader, BufWriter};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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
    lockfile: BufWriter<File>,
    flags: LockfileFlags,
    path: PathBuf,
    lockfile_path: PathBuf,
    committed: AtomicBool,
    rolled_back: AtomicBool,
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
        assert!(
            !path.exists() || path.is_file(),
            "cannot create lock on symlinks or directories `{}`",
            path.display()
        );
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
            flags,
            lockfile_path,
            lockfile: BufWriter::new(lockfile),
            path: path.to_path_buf(),
            committed: Default::default(),
            rolled_back: Default::default(),
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
    fn commit(&mut self) -> io::Result<()> {
        // ignore commit after a rollback
        if self.rolled_back.load(Ordering::Acquire) {
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
        self.committed.store(true, Ordering::Relaxed);

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
        self.rolled_back.store(true, Ordering::Relaxed);
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        // can't be both rolled_back and committed
        assert!(
            !self.rolled_back.load(Ordering::Relaxed) || !self.committed.load(Ordering::Relaxed)
        );
        // if either explicitly rolled back, or not explicitly committed, then rollback
        if self.rolled_back.load(Ordering::Relaxed) || !self.committed.load(Ordering::Relaxed) {
            self.cleanup().unwrap();
        }
    }
}

/// the default is `commit`, `rollback` must be explicit
/// `data` *must not* have interior mutability otherwise changes may be ignored
/// as the `dirty` flag will not be set
#[derive(Debug)]
pub struct Filelock<T: Serialize> {
    data: T,
    lockfile: Lockfile,
    dirty: bool,
    rolled_back: AtomicBool,
}

impl<T: Serialize + Deserialize + Default> Filelock<T> {
    pub fn lock_with_flags(path: impl AsRef<Path>, flags: LockfileFlags) -> BitResult<Self> {
        let mut lockfile = Lockfile::open(path, flags)?;
        let data = match &mut lockfile.file {
            Some(file) => T::deserialize(BufReader::new(file))?,
            None => T::default(),
        };
        Ok(Filelock { lockfile, data, dirty: false, rolled_back: Default::default() })
    }

    pub fn lock(path: impl AsRef<Path>) -> BitResult<Self> {
        Self::lock_with_flags(path, LockfileFlags::empty())
    }
}

impl<T: Serialize> Filelock<T> {
    pub fn rollback(&self) {
        self.rolled_back.store(true, Ordering::Relaxed);
        self.lockfile.rollback();
    }
}

impl<T: Serialize> Drop for Filelock<T> {
    fn drop(&mut self) {
        if self.rolled_back.load(Ordering::Relaxed) || !self.dirty {
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
        debug_assert!(!self.rolled_back.load(Ordering::Relaxed));
        &self.data
    }
}

impl<T: Serialize> DerefMut for Filelock<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // NOTE: conservatively assume any mutable access to the inner type results in a change
        // this makes it somewhat important for efficiency to only take `mut self` if necessary
        // this is conservative in the sense it assumes chances are made if `inner` is borrowed mutably
        // furthermore, we cannot use interior mutability anywhere in `BitIndexInner`
        // also, all data in `BitIndex` must just be metadata that is not persisted otherwise that will be lost
        debug_assert!(!self.rolled_back.load(Ordering::Relaxed));
        self.dirty = true;
        &mut self.data
    }
}

#[cfg(test)]
mod tests;
