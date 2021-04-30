use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry};
use crate::iter::BitIterator;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

pub trait Differ<'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r>;
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
pub struct GenericDiff<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitIterator,
    J: BitIterator,
{
    differ: &'d mut D,
    old_iter: Peekable<Fuse<I>>,
    new_iter: Peekable<Fuse<J>>,
    pd: std::marker::PhantomData<&'r ()>,
}

#[derive(Debug)]
enum Changed {
    Yes,
    No,
    Maybe,
}

impl<'r> BitIndex<'r> {
    /// determine's whether `new` is *definitely* different from `old`
    // (preferably without comparing hashes)
    pub fn has_changes(&self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<bool> {
        trace!("BitIndex::has_changes({} -> {})?", old.filepath, new.filepath);
        // should only be comparing the same file
        assert_eq!(old.filepath, new.filepath);
        // the "old" entry should always have a calculated hash
        assert!(old.hash.is_known());
        match self.has_changes_inner(old, new)? {
            Changed::Yes => Ok(true),
            Changed::No => Ok(false),
            Changed::Maybe => {
                // file may have changed, but we are not certain, so check the hash
                let mut new_hash = new.hash;
                if new_hash.is_zero() {
                    new_hash = self.repo.hash_blob(new.filepath)?;
                }

                let changed = old.hash != new_hash;
                eprintln!("{}", old.filepath);
                if !changed {
                    // TODO update index entries so we don't hit this path again
                    // maybe the parameters to this function need to be less general
                    // and rather than be `old` and `new` needs to be `index_entry` and `worktree_entry
                    self.update_stat(*new)?;
                }
                Ok(changed)
            }
        }
    }

    fn update_stat(&self, entry: BitIndexEntry) -> BitResult<()> {
        Ok(())
        // self.add_entry(entry)
    }

    /// determines whether two index_entries are definitely different
    /// `new` should be the "old" entry, and `other` should be the "new" one
    fn has_changes_inner(&self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<Changed> {
        if self.is_racy_entry(old) {
            debug!("racy entry {}", new.filepath);
            return Ok(Changed::Maybe);
        }

        //? check assume_unchanged and skip_worktree here?
        if old.hash == new.hash {
            debug!("{} unchanged: hashes match {} {}", old.filepath, old.hash, new.hash);
            return Ok(Changed::No);
        }

        if old.mtime == new.mtime {
            debug!("{} unchanged: non-racy mtime match {} {}", old.filepath, old.mtime, new.mtime);
            return Ok(Changed::No);
        }

        if old.filesize != new.filesize {
            debug!("{} changed: filesize {} -> {}", old.filepath, old.filesize, new.filesize);
            return Ok(Changed::Yes);
        }

        if old.inode != new.inode {
            debug!("{} changed: inode {} -> {}", old.filepath, old.inode, new.inode);
            return Ok(Changed::Yes);
        }

        if tls::with_config(|config| config.filemode())? && old.mode != new.mode {
            debug!("{} changed: filemode {} -> {}", old.filepath, old.mode, new.mode);
            return Ok(Changed::Yes);
        }

        debug!("{} uncertain if changed", old.filepath);

        Ok(Changed::Maybe)
    }
}

impl<'d, 'r, D, I, J> GenericDiff<'d, 'r, D, I, J>
where
    D: Differ<'r>,
    I: BitIterator,
    J: BitIterator,
{
    fn new(differ: &'d mut D, old_iter: I, new_iter: J) -> Self {
        Self {
            old_iter: old_iter.fuse().peekable(),
            new_iter: new_iter.fuse().peekable(),
            differ,
            pd: std::marker::PhantomData,
        }
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
        if self.differ.index_mut().has_changes(&old, &new)? {
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
