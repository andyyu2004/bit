use super::*;

pub struct WorktreeTreeIter {}

impl FallibleIterator for WorktreeTreeIter {
    type Error = BitGenericError;
    type Item = BitIndexEntry;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        todo!()
    }
}
