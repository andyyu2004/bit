use crate::error::{BitError, BitGenericError, BitResult};
use crate::index::{BitIndexEntry, BitIndexEntryFlags};
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use fallible_iterator::{Fuse, Peekable};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitDiff {}

struct DiffBuilder<O, N>
where
    O: FallibleIterator<Item = BitIndexEntry>,
    N: FallibleIterator<Item = BitIndexEntry>,
{
    old_iter: Peekable<Fuse<O>>,
    new_iter: Peekable<Fuse<N>>,
}

impl<O, N> DiffBuilder<O, N>
where
    O: FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>,
    N: FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>,
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
                    if old < new {
                        self.handle_deleted_record(old)?
                    } else if old > new {
                        self.handle_created_record(new)?
                    } else {
                        self.handle_updated_record(old, new)?
                    };
                }
            };
        }
        todo!()
    }
}

impl BitRepo {
    pub fn diff_worktree_index(&self) -> BitResult<BitDiff> {
        self.with_index(|index| self.diff_from_iterators(index.iter(), self.worktree_iter()))
    }

    /// both iterators must be sorted by path
    pub fn diff_from_iterators(
        &self,
        old_iter: impl FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>,
        new_iter: impl FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>,
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
