use crate::cmd::BitHashObjectOpts;
use crate::error::BitResult;
use crate::index::{BitIndexEntry, BitIndexEntryFlags};
use crate::obj::{BitObjType, FileMode};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::tls;
use ignore::Walk;
use std::fs::DirEntry;
use std::io;
use std::os::linux::fs::MetadataExt;
use std::path::{Path, PathBuf};

struct BitDiff {}

struct WorkdirIter {
    iter: Walk,
}

impl WorkdirIter {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { iter: Walk::new(path) }
    }
}

impl Iterator for WorkdirIter {
    type Item = BitIndexEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO is it better to have a fallible iterator
        // or just unwrap everything in here
        let direntry = self.iter.next().transpose().unwrap()?;
        let path = direntry.path();
        let metadata = direntry.metadata().unwrap();
        let index_entry = BitIndexEntry {
            ctime_sec: metadata.st_ctime() as u32,
            ctime_nano: metadata.st_ctime_nsec() as u32,
            mtime_sec: metadata.st_mtime() as u32,
            mtime_nano: metadata.st_mtime_nsec() as u32,
            device: metadata.st_dev() as u32,
            inode: metadata.st_ino() as u32,
            mode: FileMode::new(metadata.st_mode()),
            uid: metadata.st_uid(),
            gid: metadata.st_gid(),
            filesize: metadata.st_size() as u32,
            hash: tls::REPO.with(|repo| {
                repo.bit_hash_object(BitHashObjectOpts {
                    objtype: BitObjType::Blob,
                    do_write: false,
                    path: path.to_path_buf(),
                })
                .unwrap()
            }),
            flags: BitIndexEntryFlags::default(),
            filepath: BitPath::intern(path),
        };
        Some(index_entry)
    }
}

impl BitRepo {
    fn diff_from_iterators(
        &self,
        old_iter: impl Iterator<Item = BitIndexEntry>,
        new_iter: impl Iterator<Item = BitIndexEntry>,
    ) -> BitResult<BitDiff> {
        todo!()
    }
}
