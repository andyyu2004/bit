use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;

const CHUNK_SIZE: usize = 16;

#[derive(Default)]
struct DeltaIndex<'s> {
    source: &'s [u8],
    indices: HashMap<&'s [u8; CHUNK_SIZE], usize>,
}

impl<'s> Debug for DeltaIndex<'s> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // convert the bytes to str just for debugging
        let map = self
            .indices
            .iter()
            // SAFETY only using the output for printing to terminal
            .map(|(&k, &v)| (unsafe { std::str::from_utf8_unchecked(k) }, v))
            .collect::<HashMap<&'s str, usize>>();
        map.fmt(f)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum DeltaOp<'s> {
    /// copy (offset, length)
    Copy(usize, usize),
    Insert(&'s [u8]),
}

impl<'s> DeltaIndex<'s> {
    pub fn new(source: &'s [u8]) -> Self {
        let indices = source
            .chunks_exact(CHUNK_SIZE)
            .enumerate()
            .map(|(i, x)| (x.try_into().expect("chunk was of wrong size"), i * CHUNK_SIZE))
            .collect();
        Self { source, indices }
    }

    pub fn compress(&self, target: &'s [u8]) -> Vec<DeltaOp<'s>> {
        DeltaIndexCompressor::new(self, target).compress()
    }
}

#[derive(Debug)]
struct DeltaIndexCompressor<'a, 's> {
    delta_index: &'a DeltaIndex<'s>,
    target: &'s [u8],
    /// insert buffer
    insert: &'s [u8],
    /// current index into target slice
    target_idx: usize,
    ops: Vec<DeltaOp<'s>>,
}

impl<'a, 's> DeltaIndexCompressor<'a, 's> {
    pub fn new(delta_index: &'a DeltaIndex<'s>, target: &'s [u8]) -> Self {
        Self { delta_index, target, target_idx: 0, insert: b"", ops: Default::default() }
    }

    /// returns the current slice of the target
    #[inline]
    fn slice(&self) -> &'s [u8; 16] {
        self.source[self.target_idx..self.target_idx + CHUNK_SIZE].try_into().unwrap()
    }

    fn expand_left(&mut self, mut source_idx: usize, mut target_idx: usize) -> usize {
        debug_assert_eq!(self.source[source_idx], self.target[target_idx]);
        while self.source[target_idx - 1] == self.target[target_idx - 1] {
            source_idx -= 1;
            target_idx -= 1;
            self.insert = &self.insert[..self.insert.len() - 1];
        }
        source_idx
    }

    fn expand_right(&mut self, mut source_idx: usize, mut target_idx: usize) -> usize {
        debug_assert_eq!(self.source[source_idx - 1], self.target[target_idx - 1]);
        while self.source[source_idx] == self.target[target_idx] {
            source_idx += 1;
            target_idx += 1;
        }
        source_idx
    }

    /// returns range into the source slice
    fn expand_match(&mut self, source_idx: usize, target_idx: usize) -> (usize, usize) {
        let l = self.expand_left(source_idx, target_idx);
        let r = self.expand_right(source_idx + CHUNK_SIZE, target_idx + CHUNK_SIZE);
        (l, r)
    }

    // everything here is wrong (including anything it calls transitively)
    fn compress(mut self) -> Vec<DeltaOp<'s>> {
        loop {
            let slice = self.slice();
            dbg!(unsafe { std::str::from_utf8_unchecked(slice) });
            if let Some(&source_idx) = self.indices.get(slice) {
                let (start, end) = self.expand_match(source_idx, self.target_idx);
                self.ops.push(DeltaOp::Insert(std::mem::take(&mut self.insert)));
                self.ops.push(DeltaOp::Copy(start, end - start));
                self.target_idx = end;
            } else {
                self.insert = slice;
                self.target_idx += 1;
            }
        }
    }
}

impl<'s> Deref for DeltaIndexCompressor<'_, 's> {
    type Target = DeltaIndex<'s>;

    fn deref(&self) -> &Self::Target {
        &self.delta_index
    }
}

#[cfg(test)]
mod tests;
