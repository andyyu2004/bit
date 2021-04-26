use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::iter::BitIterator;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

pub trait Differ {
    fn on_created(&mut self, _new: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }

    fn on_modified(&mut self, _old: BitIndexEntry, _new: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }

    fn on_deleted(&mut self, _old: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }
}
pub struct GenericDiff<'d, D, I, J>
where
    D: Differ,
    I: BitIterator,
    J: BitIterator,
{
    differ: &'d mut D,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
}

impl BitIndexEntry {
    /// determines whether two index_entries are definitely different
    /// `self` should be the "old" entry, and `other` should be the "new" one
    pub fn has_changed(&self, other: &Self) -> BitResult<bool> {
        // the "old" entry should always have a calculated hash
        assert!(self.hash.is_known());
        //? there are probably some problems with this current implementation
        //? check assume_unchanged and skip_worktree here?
        // the following condition causes some non deterministic results (due to racy git)?
        // if self.mtime_sec == other.mtime_sec && self.mtime_nano == other.mtime_nano {
        // return Ok(false);
        // }
        // for now we just ignore the mtime and just consider the other values
        // this conservative approach is probably generally good enough as
        // its unlikely that after changing a file that the size will be exactly the same
        // actually there are similar issues with ctime as well which is odd, unsure of the cause of either
        // so will just avoiding using either for now :)
        if self.hash == other.hash {
            return Ok(false);
        }
        if self.filesize != other.filesize
            || self.inode != other.inode
            || self.filepath != other.filepath
            || tls::with_config(|config| config.filemode())? && self.mode != other.mode
        {
            return Ok(true);
        }

        // file may have changed, but we are not certain, so check the hash

        dbg!("bad path");
        let mut other_hash = other.hash;
        if other_hash.is_zero() {
            other_hash = tls::REPO.with(|repo| repo.hash_blob(other.filepath))?;
        }

        let changed = self.hash != other_hash;
        if !changed {
            // TODO update index entries so we don't hit this path again
            // maybe the parameters to this function need to be less general
            // and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
            // tls::with_index(|index| index.update_stat())
        }
        Ok(changed)
    }
}

impl<'d, D, I, J> GenericDiff<'d, D, I, J>
where
    D: Differ,
    I: BitIterator,
    J: BitIterator,
{
    fn new(differ: &'d mut D, old_iter: I, new_iter: J) -> Self {
        Self { old_iter: old_iter.fuse().peekable(), new_iter: new_iter.fuse().peekable(), differ }
    }

    pub fn run(differ: &'d mut D, old_iter: I, new_iter: J) -> BitResult<()> {
        Self::new(differ, old_iter, new_iter).diff_generic()
    }

    fn handle_delete(&mut self, old: BitIndexEntry) -> BitResult<()> {
        self.old_iter.next()?;
        self.differ.on_deleted(old)
    }

    fn handle_create(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.new_iter.next()?;
        self.differ.on_created(new)
    }

    fn handle_potential_update(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        self.new_iter.next()?;
        // if we are here then we know that the path and stage of the entries match
        // however, that does not mean that the file has not changed
        if old.has_changed(&new)? {
            self.differ.on_modified(old, new)?;
        }
        Ok(())
    }

    pub fn diff_generic(&mut self) -> BitResult<()> {
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(&new)) => self.handle_create(new)?,
                (Some(&old), None) => self.handle_delete(old)?,
                (Some(&old), Some(&new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match old.cmp(&new) {
                        Ordering::Less => self.handle_delete(old)?,
                        Ordering::Equal => self.handle_potential_update(old, new)?,
                        Ordering::Greater => self.handle_create(new)?,
                    }
                }
            };
        }
        Ok(())
    }
}

impl BitRepo {
    pub fn diff_worktree_index(&self) -> BitResult<BitDiff> {
        self.with_index(|index| self.diff_from_iterators(index.iter(), self.worktree_iter()?))
    }

    /// both iterators must be sorted by path
    pub fn diff_from_iterators(
        &self,
        old_iter: impl BitIterator,
        new_iter: impl BitIterator,
    ) -> BitResult<BitDiff> {
        todo!()
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
