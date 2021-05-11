use crate::error::BitResult;
use crate::io::{BufReadExt, BufReadExtSized, ReadExt};
use crate::serialize::{Deserialize, DeserializeSized};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{self, Debug, Formatter};
use std::io::{BufRead, Read};
use std::ops::Deref;

const CHUNK_SIZE: usize = 16;

#[derive(PartialEq, Clone, Debug)]
pub struct Delta {
    source_size: u64,
    target_size: u64,
    ops: Vec<DeltaOp>,
}

impl Delta {
    pub fn expand(&self, bytes: impl AsRef<[u8]>) -> BitResult<Vec<u8>> {
        trace!(
            "Delta::expand(bytes: ...) (source_size: {} -> target_size: {})",
            self.source_size,
            self.target_size
        );
        let bytes = bytes.as_ref();
        ensure_eq!(
            self.source_size as usize,
            bytes.len(),
            "expected source size to be `{}`, but given source with size `{}`",
            self.source_size,
            bytes.len()
        );

        let mut expanded = Vec::with_capacity(self.target_size as usize);
        for op in &self.ops {
            let slice = match op {
                &DeltaOp::Copy(offset, size) => {
                    let (offset, size) = (offset as usize, size as usize);
                    &bytes[offset..offset + size]
                }
                DeltaOp::Insert(slice) => slice,
            };
            expanded.extend_from_slice(slice)
        }

        ensure_eq!(
            self.target_size as usize,
            expanded.len(),
            "expected target size to be `{}`, but got expanded target with size `{}`",
            self.target_size,
            expanded.len()
        );

        Ok(expanded)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeltaOp {
    /// copy (offset, size)
    Copy(u64, u64),
    Insert(Vec<u8>),
}

impl Deserialize for DeltaOp {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        // the MSB of the first byte tells us whether it is a
        // `Copy` or `Insert` instruction
        let byte = reader.read_u8()?;
        if byte & 0x80 != 0 {
            let n = reader.read_le_packed(byte)?;
            // assert highest byte is zero
            debug_assert_eq!(n & 0xFF << 56, 0);
            let (offset, mut size) = (n & 0xFFFFFFFF, n >> 32);
            // 16 is default value for size
            if size == 0 {
                size = 16
            }
            Ok(Self::Copy(offset, size))
        } else {
            reader.read_vec::<u8>(byte as usize & 0x7f).map(Self::Insert)
        }
    }
}

impl DeserializeSized for Delta {
    fn deserialize_sized(r: &mut impl BufRead, size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let source_size = r.read_size()?;
        let target_size = r.read_size()?;
        trace!(
            "Delta::deserialize_sized(size: {}); source_size: {}; target_size: {}",
            size,
            source_size,
            target_size
        );
        let r = &mut r.take(size);
        //? size is definitely an overestimate but maybe its fine
        let mut ops = Vec::with_capacity(size as usize);

        while !r.is_at_eof()? {
            ops.push(DeltaOp::deserialize(r)?);
        }

        Ok(Self { source_size, target_size, ops })
    }
}

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

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DeltaOpSlice<'s> {
    /// copy (offset, length)
    // TODO match the types with the nonslice DeltaOp
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

    pub fn compress(&self, target: &'s [u8]) -> Vec<DeltaOpSlice<'s>> {
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
    ops: Vec<DeltaOpSlice<'s>>,
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
    fn compress(mut self) -> Vec<DeltaOpSlice<'s>> {
        loop {
            let slice = self.slice();
            dbg!(unsafe { std::str::from_utf8_unchecked(slice) });
            if let Some(&source_idx) = self.indices.get(slice) {
                let (start, end) = self.expand_match(source_idx, self.target_idx);
                self.ops.push(DeltaOpSlice::Insert(std::mem::take(&mut self.insert)));
                self.ops.push(DeltaOpSlice::Copy(start, end - start));
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
