use crate::error::BitResult;
use crate::hash::{BitHash, SHA1Hash, BIT_HASH_SIZE};
use crate::io::{BufReadExt, HashReader, ReadExt};
use crate::obj::{BitId, BitObjKind};
use crate::serialize::{BufReadSeek, Deserialize};
use num_traits::ToPrimitive;
use std::io::{BufRead, SeekFrom};
use std::ops::{Deref, DerefMut};

const PACK_IDX_MAGIC: u32 = 0xff744f63;
const FANOUT_ENTRYC: usize = 256;
const FANOUT_ENTRY_SIZE: u64 = 4;
const FANOUT_SIZE: u64 = FANOUT_ENTRYC as u64 * FANOUT_ENTRY_SIZE;
const PACK_IDX_HEADER_SIZE: u64 = 8;
const CRC_SIZE: u64 = 4;
const OFFSET_SIZE: u64 = 4;
/// maximum 31 bit number (highest bit represents it uses a large offset in the EXT layer)
const MAX_OFFSET: u32 = 0x7fffffff;

#[derive(Debug)]
pub struct PackIndex {
    /// layer 1 of the fanout table
    fanout: [u32; FANOUT_ENTRYC],
    hashes: Vec<BitHash>,
    crcs: Vec<u32>,
    offsets: Vec<u32>,
    pack_hash: SHA1Hash,
}

pub struct PackIndexReader<'r, R> {
    reader: &'r mut R,
    fanout: [u32; FANOUT_ENTRYC],
    /// number of oids
    n: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, FromPrimitive, ToPrimitive)]
pub enum Layer {
    Oid = 0,
    Crc = 1,
    Ofs = 2,
    Ext = 3,
}

impl<'r, R: BufReadSeek> PackIndexReader<'r, R> {
    pub fn new(reader: &'r mut R) -> BitResult<Self> {
        PackIndex::parse_header(reader)?;
        let fanout = reader.read_array::<u32, FANOUT_ENTRYC>()?;
        let n = fanout[FANOUT_ENTRYC - 1] as u64;
        Ok(Self { reader, fanout, n })
    }
}

impl<'r, R: BufReadSeek> PackIndexReader<'r, R> {
    /// returns the offset of the object with oid `oid` in the packfile
    pub fn find_oid_offset(&mut self, oid: BitHash) -> BitResult<u32> {
        let index = self.find_oid_index(oid)?;
        debug_assert_eq!(oid, self.read_from(Layer::Oid, index)?);
        // let crc = self.read_from::<u32>(1, index)?;
        let offset = self.read_from::<u32>(Layer::Ofs, index)?;
        assert!(offset < MAX_OFFSET, "todo ext");
        Ok(offset)
    }

    /// returns the offset of the start of the layer relative to the start of
    /// the pack index in bytes
    pub fn offset_of(&mut self, layer: Layer, index: u64) -> u64 {
        debug_assert!(layer < Layer::Ext);
        const SIZE: [u64; 3] = [20, 4, 4];
        let layer = layer.to_usize().unwrap();
        let base = PACK_IDX_HEADER_SIZE
            + FANOUT_SIZE
            + (0..layer).map(|layer| SIZE[layer] * self.n).sum::<u64>();
        base + index * SIZE[layer]
    }

    pub fn read_from<T: Deserialize>(&mut self, layer: Layer, index: u64) -> BitResult<T> {
        let offset = self.offset_of(layer, index);
        self.seek(SeekFrom::Start(offset))?;
        self.read_type()
    }

    /// return the index of `oid` in the Oid layer of the packindex (unit is sizeof::<BitHash>)
    fn find_oid_index(&mut self, oid: BitHash) -> BitResult<u64> {
        // fanout has 256 elements
        // example
        // [
        //     2,
        //     4,
        //     5,
        //     7,
        //     11,
        //     18
        //     ...
        //     n
        // ]
        // sorted list of n hashes
        //     00....
        //     00....
        //     01....
        //     01....
        //     02....
        //     03....
        //     03....
        //
        let prefix = oid[0] as usize;
        // low..high (inclusive lower bound, exclusive upper bound)
        let low = if prefix == 0 { 0 } else { self.fanout[prefix - 1] } as u64;
        let high = self.fanout[prefix] as u64;

        self.seek(SeekFrom::Current(low as i64 * BIT_HASH_SIZE as i64))?;
        let oids = self.reader.read_vec((high - low) as usize)?;
        match oids.binary_search(&oid) {
            Ok(idx) => Ok(low + idx as u64),
            Err(..) => Err(anyhow!("oid `{}` not found in packindex", oid)),
        }
    }
}

impl<'r, R> Deref for PackIndexReader<'r, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

impl<'r, R> DerefMut for PackIndexReader<'r, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.reader
    }
}

impl Deserialize for PackIndex {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut r = HashReader::new_sha1(reader);
        Self::parse_header(&mut r)?;
        let fanout = r.read_array::<u32, FANOUT_ENTRYC>()?;
        // the last value of the layer 1 fanout table is the number of
        // hashes we expect as it is cumulative
        let n = fanout[FANOUT_ENTRYC - 1] as usize;
        let hashes = r.read_vec(n)?;
        debug_assert!(hashes.is_sorted());

        let crcs = r.read_vec::<u32>(n)?;
        let offsets = r.read_vec::<u32>(n)?;

        // TODO 8-byte offsets for large packfiles
        // let big_offsets = todo!();
        let pack_hash = r.read_bit_hash()?;
        let hash = r.finalize_sha1_hash();
        let idx_hash = r.read_bit_hash()?;

        ensure_eq!(idx_hash, hash);
        assert!(r.is_at_eof()?, "todo parse level 5 fanout for large indexes");
        Ok(Self { fanout, hashes, crcs, offsets, pack_hash })
    }
}

impl PackIndex {
    fn parse_header(reader: &mut dyn BufRead) -> BitResult<()> {
        let magic = reader.read_u32()?;
        ensure_eq!(magic, PACK_IDX_MAGIC, "invalid pack index signature");
        let version = reader.read_u32()?;
        ensure_eq!(version, 2);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
