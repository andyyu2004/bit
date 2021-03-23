use crate::cmd::BitHashObjectOpts;
use crate::error::BitResult;

use crate::index::{BitIndexEntry, BitIndexEntryFlags};
use crate::obj::{BitObjType, FileMode};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::tls;
use ignore::{Walk, WalkBuilder};

use std::iter::{Fuse, Peekable};
use std::os::linux::fs::MetadataExt;
use std::path::Path;

#[derive(Debug, Clone, Eq, PartialEq)]
struct BitDiff {}

struct WorkdirIter {
    iter: Walk,
}

impl WorkdirIter {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { iter: WalkBuilder::new(path).sort_by_file_path(Ord::cmp).build() }
    }
}

impl Iterator for WorkdirIter {
    type Item = BitIndexEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO is it better to have a fallible iterator
        // or just unwrap everything in here :)

        // ignore directories
        let direntry = loop {
            let entry = self.iter.next().transpose().unwrap()?;
            if entry.path().is_file() {
                break entry;
            };
        };

        let index_entry = tls::REPO.with(|repo| {
            let path = direntry.path();
            let relative_path = pathdiff::diff_paths(path, &repo.worktree).unwrap();
            let metadata = direntry.metadata().unwrap();
            BitIndexEntry {
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
                hash: repo
                    .bit_hash_object(BitHashObjectOpts {
                        objtype: BitObjType::Blob,
                        do_write: false,
                        path: path.to_path_buf(),
                    })
                    .unwrap(),
                flags: BitIndexEntryFlags::default(),
                filepath: BitPath::intern(relative_path),
            }
        });

        Some(index_entry)
    }
}

struct DiffBuilder<O, N>
where
    O: Iterator<Item = BitIndexEntry>,
    N: Iterator<Item = BitIndexEntry>,
{
    old_iter: Peekable<Fuse<O>>,
    new_iter: Peekable<Fuse<N>>,
}

impl<O, N> DiffBuilder<O, N>
where
    O: Iterator<Item = BitIndexEntry>,
    N: Iterator<Item = BitIndexEntry>,
{
    pub fn new(old_iter: O, new_iter: N) -> Self {
        Self { old_iter: old_iter.fuse().peekable(), new_iter: new_iter.fuse().peekable() }
    }

    fn handle_deleted_record(&mut self, old: BitIndexEntry) {
        println!("deleted {:?}", old.filepath);
        self.old_iter.next();
    }

    fn handle_created_record(&mut self, new: BitIndexEntry) {
        println!("created {:?}", new.filepath);
        self.new_iter.next();
    }

    fn handle_updated_record(&mut self, old: BitIndexEntry, new: BitIndexEntry) {
        debug_assert_eq!(old.filepath, new.filepath);
        println!("updated {:?}", new.filepath);
        self.old_iter.next();
        self.new_iter.next();
    }

    fn build_diff(&mut self) -> BitResult<BitDiff> {
        loop {
            match (self.old_iter.peek(), self.new_iter.peek()) {
                (None, None) => break,
                (None, Some(&new)) => self.handle_created_record(new),
                (Some(&old), None) => self.handle_deleted_record(old),
                (Some(&old), Some(&new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    if old < new {
                        self.handle_deleted_record(old)
                    } else if old > new {
                        self.handle_created_record(new)
                    } else {
                        self.handle_updated_record(old, new)
                    };
                }
            };
        }
        todo!()
    }
}

impl BitRepo {
    fn workdir_iterator(&self) -> impl Iterator<Item = BitIndexEntry> {
        WorkdirIter::new(&self.worktree)
    }

    fn diff_workdir_index(&self) -> BitResult<BitDiff> {
        self.with_index(|index| self.diff_from_iterators(index.iter(), self.workdir_iterator()))
    }

    /// both iterators must be sorted by path
    fn diff_from_iterators(
        &self,
        old_iter: impl Iterator<Item = BitIndexEntry>,
        new_iter: impl Iterator<Item = BitIndexEntry>,
    ) -> BitResult<BitDiff> {
        DiffBuilder::new(old_iter, new_iter).build_diff()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn simple_diff() -> BitResult<()> {
        Ok(())
        // BitRepo::find("tests/repos/difftest", |repo| {
        //     let diff = repo.diff_workdir_index()?;
        //     dbg!(diff);
        //     Ok(())
        // })
    }
}
