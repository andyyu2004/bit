use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry};
use crate::iter::BitIterator;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

pub trait Differ {
    fn has_changes(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<bool>;
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

impl<'r> BitIndex<'r> {
    pub fn has_changed(&self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        if !self.has_maybe_changed(old, new)? {
            // definitely has not changed
            return Ok(false);
        }

        dbg!("bad path");
        let mut new_hash = new.hash;
        if new_hash.is_zero() {
            new_hash = tls::REPO.with(|repo| repo.hash_blob(new.filepath))?;
        }

        let changed = old.hash != new_hash;
        if !changed {
            // TODO update index entries so we don't hit this path again
            // maybe the parameters to this function need to be less general
            // and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
            // tls::with_index(|index| index.update_stat())
        }
        Ok(changed)
    }

    /// determines whether two index_entries are definitely different
    /// `new` should be the "old" entry, and `other` should be the "new" one
    pub fn has_maybe_changed(&self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        // the "old" entry should always have a calculated hash
        assert!(old.hash.is_known());

        if self.is_racy_entry(new) {
            return Ok(true);
        }

        //? there are probably some problems with this current implementation
        //? check assume_unchanged and skip_worktree here?
        if old.hash == new.hash || old.mtime == new.mtime {
            return Ok(false);
        }
        if old.filesize != new.filesize
            || old.inode != new.inode
            || old.filepath != new.filepath
            || tls::with_config(|config| config.filemode())? && old.mode != new.mode
        {
            return Ok(true);
        }
        Ok(true)

        // file may have changed, but we are not certain, so check the hash
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
        if self.differ.has_changes(old, new)? {
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
