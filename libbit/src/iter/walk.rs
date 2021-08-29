use super::*;
use std::cmp::Ordering;

impl<'rcx> BitRepo<'rcx> {
    pub fn walk_iterators<'a, const N: usize>(
        self,
        iterators: [Box<dyn BitTreeIterator + 'a>; N],
    ) -> impl BitIterator<[Option<BitIndexEntry>; N]> + 'a {
        WalkIterators::new(iterators)
    }
}

pub struct WalkIterators<'a, const N: usize> {
    iterators: [Box<dyn BitTreeIterator + 'a>; N],
}

impl<'a, const N: usize> WalkIterators<'a, N> {
    pub fn new(iterators: [Box<dyn BitTreeIterator + 'a>; N]) -> Self {
        Self { iterators }
    }
}

impl<'a, const N: usize> FallibleIterator for WalkIterators<'a, N> {
    type Error = BitGenericError;
    type Item = [Option<BitIndexEntry>; N];

    /// walk `N` iterators calling the provided callback on matching entries
    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let mut next_entries: [Option<BitIndexEntry>; N] = [None; N];
        let mut first_match = None;

        for (i, iterator) in self.iterators.iter_mut().enumerate() {
            let entry = match iterator.peek()? {
                Some(entry) => entry,
                None => continue,
            };

            // preemptively set the entry as more cases requires this than not
            next_entries[i] = Some(entry);
            match first_match {
                None => first_match = Some(entry),
                Some(fst_entry) => match entry.entry_cmp(&fst_entry) {
                    Ordering::Less => {
                        // if we found a entry that comes earlier then we forget
                        // the previous first_match and reset `next_entries`
                        next_entries = [None; N];
                        first_match = Some(entry);
                        next_entries[i] = first_match;
                    }
                    Ordering::Equal => {}
                    Ordering::Greater => {
                        // clear the preemptively set entry
                        next_entries[i] = None;
                    }
                },
            }
        }

        // all iterators should be exhausted and we are done
        if first_match.is_none() {
            for iter in &mut self.iterators {
                debug_assert!(iter.next()?.is_none());
            }
            return Ok(None);
        }

        let mut is_same_tree = true;
        let mut oid = None;

        // check if all entries are the same tree in which case we can step over the entire subtree
        // `flatten` essentially filters out all the Nones
        for entry in next_entries.iter().flatten() {
            if !entry.is_tree() {
                is_same_tree = false;
                break;
            }

            match oid {
                Some(oid) =>
                    if entry.oid() != oid {
                        is_same_tree = false;
                        break;
                    },
                None => oid = Some(entry.oid()),
            }
        }

        // advance iterators that were used
        for (i, entry) in next_entries.iter().enumerate() {
            if entry.is_some() {
                let iter = &mut self.iterators[i];
                if is_same_tree {
                    iter.over()?;
                } else {
                    iter.next()?;
                }
            }
        }

        Ok(Some(next_entries))
    }
}
