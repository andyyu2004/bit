use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::iter::BitIterator;
use crate::repo::BitRepo;
use fallible_iterator::{Fuse, Peekable};
use std::cmp::Ordering;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

struct DiffBuilder<O, N>
where
    O: BitIterator,
    N: BitIterator,
{
    old_iter: Peekable<Fuse<O>>,
    new_iter: Peekable<Fuse<N>>,
}

impl<O, N> DiffBuilder<O, N>
where
    O: BitIterator,
    N: BitIterator,
{
    pub fn new(old_iter: O, new_iter: N) -> Self {
        Self { old_iter: old_iter.fuse().peekable(), new_iter: new_iter.fuse().peekable() }
    }

    fn handle_deleted_record(&mut self, old: BitIndexEntry) -> BitResult<()> {
        println!("deleted {:?}", old.filepath);
        self.old_iter.next()?;
        Ok(())
    }

    fn handle_created_record(&mut self, new: BitIndexEntry) -> BitResult<()> {
        println!("created {:?}", new.filepath);
        self.new_iter.next()?;
        Ok(())
    }

    fn handle_updated_record(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        debug_assert_eq!(old.filepath, new.filepath);
        println!("updated {:?}", new.filepath);
        self.old_iter.next()?;
        self.new_iter.next()?;
        Ok(())
    }

    fn build_diff(&mut self) -> BitResult<BitDiff> {
        loop {
            match (self.old_iter.peek()?, self.new_iter.peek()?) {
                (None, None) => break,
                (None, Some(&new)) => self.handle_created_record(new)?,
                (Some(&old), None) => self.handle_deleted_record(old)?,
                (Some(&old), Some(&new)) => {
                    // there is an old record that no longer has a matching new record
                    // therefore it has been deleted
                    match old.cmp(&new) {
                        Ordering::Less => self.handle_deleted_record(old)?,
                        Ordering::Equal => self.handle_updated_record(old, new)?,
                        Ordering::Greater => self.handle_created_record(new)?,
                    }
                }
            };
        }
        todo!()
    }
}

pub trait Differ {
    fn on_create(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }

    fn on_update(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }

    fn on_delete(&mut self, old: BitIndexEntry) -> BitResult<()> {
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
        self.differ.on_delete(old)
    }

    fn handle_create(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.new_iter.next()?;
        self.differ.on_create(new)
    }

    fn handle_potential_update(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        debug_assert_eq!(old.filepath, new.filepath);
        self.old_iter.next()?;
        self.new_iter.next()?;
        // if we are here then we know that the path and stage of the entries match
        // but that does not mean that the file has not changed
        if old != new {
            // equality check includes checks for nanosecond precision mtime (at least on my computer)
            // and more importantly compares the hashes
            // this implementation doesn't do many optimizations so we don't have issues such as
            // racily clean entries etc (I think?)
            self.differ.on_update(old, new)?;
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
