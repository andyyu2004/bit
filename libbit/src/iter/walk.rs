use super::*;
use std::cmp::Ordering;

impl<'rcx> BitRepo<'rcx> {
    pub fn walk_tree_iterators<'a, const N: usize>(
        self,
        iterators: [Box<dyn BitTreeIterator + 'a>; N],
    ) -> impl BitIterator<[Option<BitIndexEntry>; N]> + 'a {
        WalkTreeIterators::new(iterators)
    }

    pub fn walk_iterators<'a, const N: usize>(
        self,
        iterators: [Box<dyn BitEntryIterator + 'a>; N],
    ) -> impl BitIterator<[Option<BitIndexEntry>; N]> + 'a {
        WalkIterators::new(iterators)
    }
}

// common logic between the two walks
macro_rules! walk_common {
    ($self:ident) => {{
        let mut next_entries: [Option<BitIndexEntry>; N] = [None; N];
        let mut first_match = None;

        for (i, iterator) in $self.iterators.iter_mut().enumerate() {
            let entry = match iterator.peek()? {
                Some(entry) => entry,
                None => continue,
            };

            // Preemptively set the entry as more cases requires this than not
            next_entries[i] = Some(entry.clone());

            // There are some clones sprinkled here and there to make this macro work
            // for when peek yields references and peek yields values
            match first_match {
                None => first_match = Some(entry.clone()),
                Some(fst_entry) => match entry.diff_cmp(&fst_entry) {
                    Ordering::Less => {
                        // if we found a entry that comes earlier then we forget
                        // the previous first_match and reset `next_entries`
                        next_entries = [None; N];
                        first_match = Some(entry.clone());
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
            for iter in &mut $self.iterators {
                debug_assert!(iter.next()?.is_none());
            }
            return Ok(None);
        }

        next_entries
    }};
}

pub struct WalkTreeIterators<'a, const N: usize> {
    iterators: [Box<dyn BitTreeIterator + 'a>; N],
}

impl<'a, const N: usize> WalkTreeIterators<'a, N> {
    pub fn new(iterators: [Box<dyn BitTreeIterator + 'a>; N]) -> Self {
        Self { iterators }
    }
}

impl<'a, const N: usize> FallibleIterator for WalkTreeIterators<'a, N> {
    type Error = BitGenericError;
    type Item = [Option<BitIndexEntry>; N];

    /// walk `N` iterators calling the provided callback on matching entries
    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let next_entries = walk_common!(self);

        // Check if all entries are the same tree in which case we can step over the entire subtree
        // However, we don't step over the subtree if it is the only tree
        // `flatten` essentially filters out all the Nones
        let mut should_step_over = true;
        let mut count = 0;
        let mut oid = None;

        for entry in next_entries.iter().flatten() {
            if !entry.is_tree() {
                should_step_over = false;
                break;
            }

            match oid {
                Some(oid) =>
                    if entry.oid() != oid {
                        should_step_over = false;
                        break;
                    },
                None => oid = Some(entry.oid()),
            }
            count += 1;
        }

        // Advance iterators that were used
        for (i, entry) in next_entries.iter().enumerate() {
            if entry.is_some() {
                let iter = &mut self.iterators[i];
                if should_step_over && count > 1 {
                    iter.over()?;
                } else {
                    iter.next()?;
                }
            }
        }

        Ok(Some(next_entries))
    }
}

pub struct WalkIterators<'a, const N: usize> {
    iterators: [Peekable<Box<dyn BitEntryIterator + 'a>>; N],
}

impl<'a, const N: usize> WalkIterators<'a, N> {
    pub fn new(iterators: [Box<dyn BitEntryIterator + 'a>; N]) -> Self {
        Self { iterators: iterators.map(|iter| iter.peekable()) }
    }
}

impl<'a, const N: usize> FallibleIterator for WalkIterators<'a, N> {
    type Error = BitGenericError;
    type Item = [Option<BitIndexEntry>; N];

    /// Walk `N` iterators calling the provided callback on matching entries
    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        let next_entries = walk_common!(self);

        // Advance iterators that were used
        for (i, entry) in next_entries.iter().enumerate() {
            if entry.is_some() {
                let iter = &mut self.iterators[i];
                iter.next()?;
            }
        }

        Ok(Some(next_entries))
    }
}
