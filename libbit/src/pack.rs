use crate::delta::Delta;
use crate::error::{BitError, BitErrorExt, BitGenericError, BitResult, BitResultExt};
use crate::hash::{crc_of, SHA1Hash, BIT_HASH_SIZE};
use crate::io::{BufReadExt, BufReadExtSized, HashReader, ReadExt};
use crate::iter::{BitEntryIterator, BitIterator};
use crate::obj::*;
use crate::path::{BitFileStream, BitPath};
use crate::serialize::{BufReadSeek, Deserialize, DeserializeSized};
use fallible_iterator::FallibleIterator;
use num_traits::{FromPrimitive, ToPrimitive};
use std::fmt::{self, Debug, Formatter};
use std::io::{BufRead, BufReader, Read, SeekFrom};
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

#[derive(PartialEq)]
pub struct BitObjRaw {
    obj_type: BitObjType,
    bytes: Vec<u8>,
}

impl BitObjRaw {
    pub fn expand_with_delta(&self, delta: &Delta) -> BitResult<Self> {
        //? is it guaranteed that the (expanded) base of a delta is of the same type?
        let &Self { obj_type, ref bytes } = self;
        Ok(Self { obj_type, bytes: delta.expand(bytes)? })
    }
}

impl Debug for BitObjRaw {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.obj_type)
    }
}

#[derive(PartialEq)]
pub enum BitObjRawKind {
    Raw(BitObjRaw),
    Ofs(u64, Vec<u8>),
    Ref(Oid, Vec<u8>),
}

impl Debug for BitObjRawKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(raw) => write!(f, "BitObjRawKind::Raw({:?})", raw),
            Self::Ofs(offset, _) => write!(f, "BitObjRawKind::Ofs({}, ..)", offset),
            Self::Ref(oid, _) => write!(f, "BitObjRawKind::Ref({}, ..)", oid),
        }
    }
}

pub struct Pack {
    pack_reader: PackfileReader<BitFileStream>,
    idx_reader: PackIndexReader<BitFileStream>,
}

impl Pack {
    pub fn new(pack: BitPath, idx: BitPath) -> BitResult<Self> {
        let pack_reader = pack.stream().and_then(PackfileReader::new)?;
        let idx_reader = idx.stream().and_then(PackIndexReader::new)?;
        Ok(Self { pack_reader, idx_reader })
    }

    #[inline]
    pub fn pack_reader(&mut self) -> &mut PackfileReader<BitFileStream> {
        &mut self.pack_reader
    }

    #[inline]
    pub fn idx_reader(&mut self) -> &mut PackIndexReader<BitFileStream> {
        &mut self.idx_reader
    }

    pub fn obj_offset(&mut self, oid: Oid) -> BitResult<(u32, u64)> {
        self.idx_reader().find_oid_crc_offset(oid)
    }

    /// returns a list of oids that start with `prefix`
    pub fn prefix_matches(&mut self, prefix: PartialOid) -> BitResult<Vec<Oid>> {
        trace!("prefix_matches(prefix: {})", prefix);
        let extended = prefix.into_oid()?;
        let r = match self.obj_offset(extended) {
            // in the unlikely event that extending the prefix with zeroes
            // resulted in a valid oid then we can just return that as the only candidate
            Ok(..) => Ok(vec![extended]),
            Err(err) => {
                // we know `idx` is the index of the very first oid that has prefix `prefix`
                // as we extended prefix by using only zeroes
                // so we just start scanning from `idx` until the prefixes change
                let (_, idx) = err.into_obj_not_found_in_pack_index_err()?;
                self.idx_reader().oid_iter(idx).take_while(|oid| oid.has_prefix(prefix)).collect()
            }
        };
        trace!("prefix_matches(..) -> {:?}", r);
        r
    }

    pub fn obj_exists(&mut self, oid: Oid) -> BitResult<bool> {
        // TODO this pattern is a little unpleasant
        // do something about it if it pops up any more
        // maybe some magic with a different error type could work
        match self.obj_offset(oid) {
            Ok(..) => Ok(true),
            Err(err) if err.is_not_found_err() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn expand_raw_obj(
        &mut self,
        raw_kind: BitObjRawKind,
        base_offset: u64,
    ) -> BitResult<BitObjRaw> {
        trace!("expand_raw_obj(raw_kind: {:?}, base_offset: {})", raw_kind, base_offset);
        let (base, delta_bytes) = match raw_kind {
            BitObjRawKind::Raw(raw) => return Ok(raw),
            BitObjRawKind::Ofs(offset, delta) =>
                (self.read_obj_raw_at(base_offset - offset)?, delta),
            BitObjRawKind::Ref(base_oid, delta) => (self.read_obj_raw(base_oid)?, delta),
        };

        trace!("expand_raw_obj:base = base={:?}; delta_len={}", base, delta_bytes.len());
        let delta = Delta::deserialize_from_slice(&delta_bytes[..])?;
        base.expand_with_delta(&delta)
    }

    /// returns fully expanded raw object at offset
    pub fn read_obj_raw_at(&mut self, offset: u64) -> BitResult<BitObjRaw> {
        trace!("read_obj_raw_at(offset: {})", offset);
        let raw = self.pack_reader().read_obj_from_offset_raw(offset)?;
        self.expand_raw_obj(raw, offset)
    }

    /// returns fully expanded raw object with oid
    pub fn read_obj_raw(&mut self, oid: Oid) -> BitResult<BitObjRaw> {
        trace!("read_obj_raw(oid: {})", oid);
        let (crc, offset) = self.obj_offset(oid)?;
        let raw = self.read_obj_raw_at(offset)?;
        // TODO crc not checked for ofs_deltas as it doesn't use this function
        ensure_eq!(crc_of(&raw.bytes), crc, "crc doesn't match");
        Ok(raw)
    }

    pub fn read_obj_header(&mut self, oid: Oid) -> BitResult<BitObjHeader> {
        let (crc, offset) = self.obj_offset(oid)?;
        trace!("read_obj_header(oid: {}); crc={}; offset={}", oid, crc, offset);
        let header = self.read_obj_header_at(offset)?;
        assert!(!header.obj_type.is_delta());
        Ok(header)
    }

    fn read_obj_header_at(&mut self, offset: u64) -> BitResult<BitObjHeader> {
        trace!("read_obj_header_at(offset: {})", offset);
        let reader = self.pack_reader();
        let header = reader.read_header_from_offset(offset)?;
        // can assume base_header has same type
        let base_header = match header.obj_type {
            BitObjType::Commit | BitObjType::Tree | BitObjType::Blob | BitObjType::Tag =>
                return Ok(header),
            BitObjType::OfsDelta => {
                let ofs = reader.read_offset()?;
                self.read_obj_header_at(offset - ofs)
            }
            BitObjType::RefDelta => {
                let oid = self.pack_reader().read_oid()?;
                self.read_obj_header(oid)
            }
        }?;
        Ok(BitObjHeader { size: header.size, obj_type: base_header.obj_type })
    }

    pub fn read_obj(&mut self, oid: Oid) -> BitResult<BitObjKind> {
        trace!("read_obj(oid: {}) ", oid);
        let (crc, offset) = self.obj_offset(oid)?;
        trace!("read_obj(..); crc={}; offset={}", crc, offset);
        let obj = self.pack_reader().read_obj_from_offset(offset)?;
        let obj = match obj {
            BitObjKind::Blob(..)
            | BitObjKind::Commit(..)
            | BitObjKind::Tree(..)
            | BitObjKind::Tag(..) => obj,
            BitObjKind::OfsDelta(ofs_delta) => {
                let base_offset = offset - ofs_delta.offset;
                trace!(
                    "read_obj:ofs_delta {{ raw_offset={}; offset={} }}",
                    ofs_delta.offset,
                    base_offset
                );
                let raw = self.read_obj_raw_at(base_offset)?.expand_with_delta(&ofs_delta.delta)?;
                BitObjKind::from_raw(raw)?
            }
            BitObjKind::RefDelta(ref_delta) => {
                let raw =
                    self.read_obj_raw(ref_delta.base_oid)?.expand_with_delta(&ref_delta.delta)?;
                BitObjKind::from_raw(raw)?
            }
        };
        // TODO does crc include the extra bytes of ofs and delta?
        // ensure!(crc_of(obj), crc);
        Ok(obj)
    }
}

impl BitObjKind {
    pub fn from_raw(raw: BitObjRaw) -> BitResult<Self> {
        match raw.obj_type {
            BitObjType::Commit => Commit::deserialize_from_slice(&raw.bytes[..]).map(Self::Commit),
            BitObjType::Tree => Tree::deserialize_from_slice(&raw.bytes[..]).map(Self::Tree),
            BitObjType::Blob => Blob::deserialize_from_slice(&raw.bytes[..]).map(Self::Blob),
            BitObjType::Tag => Tag::deserialize_from_slice(&raw.bytes[..]).map(Self::Tag),
            BitObjType::OfsDelta | BitObjType::RefDelta =>
                unreachable!("found unexpanded raw object"),
        }
    }
}

#[derive(Debug)]
pub struct PackIndex {
    /// layer 1 of the fanout table
    fanout: [u32; FANOUT_ENTRYC],
    hashes: Vec<Oid>,
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
    pub fn find_oid_crc_offset(&mut self, oid: Oid) -> BitResult<(u32, u64)> {
        trace!("find_oid_crc_offset(oid: {})", oid);
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

    /// read from layer at index (index is not the same as byte offset)
    pub fn read_from<T: Deserialize>(&mut self, layer: Layer, index: u64) -> BitResult<T> {
        let offset = self.offset_of(layer, index);
        self.seek(SeekFrom::Start(offset))?;
        self.read_type()
    }

    pub fn read_oid_at(&mut self, index: u64) -> BitResult<Oid> {
        self.read_from(Layer::Oid, index)
    }

    pub fn oid_iter(&mut self, start: u64) -> impl BitIterator<Oid> + '_ {
        struct OidIter<'a, R> {
            reader: &'a mut PackIndexReader<R>,
            index: u64,
        }

        impl<'a, R: BufReadSeek> FallibleIterator for OidIter<'a, R> {
            type Error = BitGenericError;
            type Item = Oid;

            fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
                if self.index >= self.reader.n {
                    return Ok(None);
                }
                let r = self.reader.read_oid_at(self.index);
                self.index += 1;
                Some(r).transpose()
            }
        }

        OidIter { reader: self, index: start }
    }

    /// return the index of `oid` in the Oid layer of the packindex (unit is sizeof::<Oid>)
    fn find_oid_index(&mut self, oid: Oid) -> BitResult<u64> {
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

        self.seek(SeekFrom::Start(
            PACK_IDX_HEADER_SIZE + FANOUT_SIZE + low * BIT_HASH_SIZE as u64,
        ))?;
        let oids = self.reader.read_vec((high - low) as usize)?;
        match oids.binary_search(&oid) {
            Ok(idx) => Ok(low + idx as u64),
            Err(idx) => Err(anyhow!(BitError::ObjectNotFoundInPackIndex(oid, low + idx as u64))),
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
        let pack_hash = r.read_oid()?;
        let hash = r.finalize_sha1_hash();
        let idx_hash = r.read_oid()?;

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
    // the `size` here is the `size` shown in `git verify-pack` (not the `size-in-packfile`)
    // so the uncompressed size (i.e. we can call `take` on the zlib (decompressed) stream, rather than the compressed stream)
    // https://git-scm.com/docs/git-verify-pack
    pub fn read_pack_obj_header(&mut self) -> BitResult<BitObjHeader> {
        let (ty, size) = self.read_le_varint_with_shift(3)?;
        let obj_type = BitObjType::from_u8(ty).expect("invalid bit object type");
        Ok(BitObjHeader { obj_type, size })
    }

    fn read_compressed_obj_data(&mut self, obj_ty: BitObjType, size: u64) -> BitResult<BitObjKind> {
        let mut reader = BufReader::new(flate2::bufread::ZlibDecoder::new(&mut self.reader));
        BitObjKind::deserialize_as(&mut reader, obj_ty, size)
    }

    /// seek to `offset` and read pack object header
    pub fn read_header_from_offset(&mut self, offset: u64) -> BitResult<BitObjHeader> {
        self.seek(SeekFrom::Start(offset))?;
        self.read_pack_obj_header()
    }

    pub fn read_obj_from_offset_raw(&mut self, offset: u64) -> BitResult<BitObjRawKind> {
        trace!("read_obj_from_offset_raw(offset: {})", offset);
        let BitObjHeader { obj_type, size } = self.read_header_from_offset(offset)?;
        // the delta types have only the delta compressed but the size/baseoid is not,
        // the 4 base object types have all their data compressed
        // we so we have to treat them a bit differently
        match obj_type {
            BitObjType::Commit | BitObjType::Tree | BitObjType::Blob | BitObjType::Tag =>
                Ok(BitObjRawKind::Raw(BitObjRaw {
                    obj_type,
                    bytes: self.into_zlib_decode_stream().take(size).read_to_vec()?,
                })),
            BitObjType::OfsDelta => Ok(BitObjRawKind::Ofs(
                self.read_offset()?,
                self.into_zlib_decode_stream().take(size).read_to_vec()?,
            )),
            BitObjType::RefDelta => Ok(BitObjRawKind::Ref(
                self.read_oid()?,
                self.into_zlib_decode_stream().take(size).read_to_vec()?,
            )),
        }
    }

    pub fn read_obj_from_offset(&mut self, offset: u64) -> BitResult<BitObjKind> {
        let BitObjHeader { obj_type, size } = self.read_header_from_offset(offset)?;
        // the delta types have only the delta compressed but the size/baseoid is not,
        // the 4 base object types have all their data compressed
        // we so we have to treat them a bit differently
        match obj_type {
            BitObjType::Commit | BitObjType::Tree | BitObjType::Blob | BitObjType::Tag =>
                self.read_compressed_obj_data(obj_type, size),
            BitObjType::OfsDelta | BitObjType::RefDelta =>
                BitObjKind::deserialize_as(&mut self.reader, obj_type, size),
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
