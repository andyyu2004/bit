use crate::error::{BitError, BitResult};
use crate::hash::{BitHash, SHA1Hash, BIT_HASH_SIZE};
use crate::io::{BufReadExt, BufReadExtSized, HashReader, ReadExt};
use crate::obj::*;
use crate::serialize::{BufReadSeek, Deserialize, DeserializeSized};
use num_traits::{FromPrimitive, ToPrimitive};
use std::io::{BufRead, BufReader, SeekFrom};
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

pub struct PackIndexReader<R> {
    reader: R,
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

impl<R: BufReadSeek> PackIndexReader<R> {
    pub fn new(mut reader: R) -> BitResult<Self> {
        PackIndex::parse_header(&mut reader)?;
        let fanout = reader.read_array::<u32, FANOUT_ENTRYC>()?;
        let n = fanout[FANOUT_ENTRYC - 1] as u64;
        Ok(Self { reader, fanout, n })
    }
}

impl<R: BufReadSeek> PackIndexReader<R> {
    /// returns the offset of the object with oid `oid` in the packfile
    pub fn find_oid_crc_offset(&mut self, oid: BitHash) -> BitResult<(u32, u64)> {
        let index = self.find_oid_index(oid)?;
        debug_assert_eq!(oid, self.read_from(Layer::Oid, index)?);
        let crc = self.read_from::<u32>(Layer::Crc, index)?;
        let offset = self.read_from::<u32>(Layer::Ofs, index)?;
        assert!(offset < MAX_OFFSET, "todo ext");
        Ok((crc, offset as u64))
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
            Err(idx) => Err(anyhow!(BitError::ObjectNotFoundInPackIndex(oid, idx))),
        }
    }
}

impl<R> Deref for PackIndexReader<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

impl<R> DerefMut for PackIndexReader<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.reader
    }
}

impl Deserialize for PackIndex {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
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

pub struct Packfile {}

impl PackIndex {
    fn parse_header(reader: &mut impl BufRead) -> BitResult<()> {
        let magic = reader.read_u32()?;
        ensure_eq!(magic, PACK_IDX_MAGIC, "invalid pack index signature");
        let version = reader.read_u32()?;
        ensure_eq!(version, 2);
        Ok(())
    }
}

pub struct PackfileReader<R> {
    reader: R,
    n: u32,
}

impl Packfile {
    fn parse_header(reader: &mut impl BufRead) -> BitResult<u32> {
        let sig = reader.read_array::<u8, 4>()?;
        ensure_eq!(&sig, b"PACK", "invalid packfile header");
        let version = reader.read_u32()?;
        ensure_eq!(version, 2, "invalid packfile version `{}`", version);
        Ok(reader.read_u32()?)
    }
}

impl<R: BufReadSeek> PackfileReader<R> {
    pub fn new(mut reader: R) -> BitResult<Self> {
        let n = Packfile::parse_header(&mut reader)?;
        Ok(Self { reader, n })
    }

    // 3 bits object type
    // MSB is 1 then read next byte
    pub fn read_pack_obj_header(&mut self) -> BitResult<(BitObjType, u64)> {
        let n = self.read_le_varint()?;
        let ty = BitObjType::from_u64(n & 0b111).expect("invalid bit object type");
        let size = n >> 3;
        Ok((ty, size))
    }

    fn read_compressed_obj_data(&mut self, obj_ty: BitObjType, size: u64) -> BitResult<BitObjKind> {
        let mut reader = BufReader::new(flate2::bufread::ZlibDecoder::new(&mut self.reader));
        BitObjKind::deserialize_as_kind(&mut reader, obj_ty, size)
    }

    pub fn read_obj_from_offset(&mut self, offset: u64) -> BitResult<BitObjKind> {
        self.seek(SeekFrom::Start(offset))?;
        let (obj_ty, size) = self.read_pack_obj_header()?;
        // the delta types have only the delta compressed but the size/baseoid is not,
        // the 4 base object types have all their data compressed
        // we so we have to treat them a bit differently
        match obj_ty {
            BitObjType::Commit | BitObjType::Tree | BitObjType::Blob | BitObjType::Tag =>
                self.read_compressed_obj_data(obj_ty, size),
            BitObjType::OfsDelta | BitObjType::RefDelta =>
                BitObjKind::deserialize_as_kind(&mut self.reader, obj_ty, size),
        }
    }
}

impl<R> Deref for PackfileReader<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

impl<R> DerefMut for PackfileReader<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.reader
    }
}

#[cfg(test)]
mod tests;
