mod indexer;
mod writer;

pub use self::indexer::{IndexPackOpts, PackIndexer};
pub(crate) use self::writer::PackWriter;

use crate::delta::Delta;
use crate::error::{BitError, BitErrorExt, BitGenericError, BitResult, BitResultExt};
use crate::hash::{Crc32, MakeHash, SHA1Hash, OID_SIZE};
use crate::io::*;
use crate::iter::BitIterator;
use crate::obj::*;
use crate::serialize::{BufReadSeek, Deserialize, DeserializeSized, Serialize};
use fallible_iterator::FallibleIterator;
use flate2::{Decompress, FlushDecompress};
use num_traits::{FromPrimitive, ToPrimitive};
use rustc_hash::FxHashMap;
use std::collections::hash_map::RawEntryMut;
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, SeekFrom, Write};
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::path::Path;

pub const PACK_SIGNATURE: &[u8; 4] = b"PACK";
pub const PACK_EXT: &str = "pack";
pub const PACK_IDX_EXT: &str = "idx";
const PACK_IDX_MAGIC: u32 = 0xff744f63;
const FANOUT_ENTRYC: usize = 256;
const FANOUT_ENTRY_SIZE: u64 = 4;
const FANOUT_SIZE: u64 = FANOUT_ENTRYC as u64 * FANOUT_ENTRY_SIZE;
const PACK_IDX_HEADER_SIZE: u64 = 8;
const CRC_SIZE: u64 = 4;
const OFFSET_SIZE: u64 = 4;
const EXT_OFFSET_SIZE: u64 = 8;
/// maximum 31 bit number (highest bit represents it uses a large offset in the EXT layer)
const MAX_OFFSET: u64 = 0x7fffffff;

impl BitPackObjRaw {
    fn expand_with_delta_bytes(&self, delta_bytes: &[u8]) -> BitResult<Self> {
        let delta = Delta::deserialize_from_slice(&delta_bytes)?;
        self.expand_with_delta(&delta)
    }

    fn expand_with_delta(&self, delta: &Delta) -> BitResult<Self> {
        trace!("BitObjRaw::expand_with_delta(..)");
        //? is it guaranteed that the (expanded) base of a delta is of the same type?
        let &Self { obj_type, ref bytes } = self;
        Ok(Self { obj_type, bytes: delta.expand(bytes)? })
    }
}

// all the bytes of the delta in `Self::Ofs` and `Self::Ref` should be zlib-inflated already
pub enum BitPackObjRawDeltified {
    Raw(BitPackObjRaw),
    Ofs(u64, Vec<u8>),
    Ref(Oid, Vec<u8>),
}

impl Debug for BitPackObjRawDeltified {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(raw) => write!(f, "BitObjRawKind::Raw({:?})", raw),
            Self::Ofs(offset, _) => write!(f, "BitObjRawKind::Ofs({}, ..)", offset),
            Self::Ref(oid, _) => write!(f, "BitObjRawKind::Ref({}, ..)", oid),
        }
    }
}

pub struct Pack {
    pack_reader: PackfileReader<BufferedFileStream>,
    idx_reader: PackIndexReader<BufferedFileStream>,
    pack_obj_cache: FxHashMap<u64, BitPackObjRaw>,
}

impl Pack {
    pub fn new(pack: impl AsRef<Path>, idx: impl AsRef<Path>) -> BitResult<Self> {
        let pack_reader = File::open(pack)
            .map(BufReader::new)
            .map_err(Into::into)
            .and_then(PackfileReader::new)?;
        let idx_reader = File::open(idx)
            .map(BufReader::new)
            .map_err(Into::into)
            .and_then(PackIndexReader::new)?;
        Ok(Self { pack_reader, idx_reader, pack_obj_cache: Default::default() })
    }

    #[inline]
    pub fn pack_reader(&mut self) -> &mut PackfileReader<BufferedFileStream> {
        &mut self.pack_reader
    }

    #[inline]
    pub fn idx_reader(&mut self) -> &mut PackIndexReader<BufferedFileStream> {
        &mut self.idx_reader
    }

    #[inline]
    pub fn obj_crc_offset(&mut self, oid: Oid) -> BitResult<(u32, u64)> {
        self.idx_reader().find_oid_crc_offset(oid)
    }

    #[inline]
    pub fn obj_offset(&mut self, oid: Oid) -> BitResult<u64> {
        self.obj_crc_offset(oid).map(|(_crc, offset)| offset)
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
                trace!("Pack::prefix_matches: prefix not found, searching for candidates");
                let (_, idx) = err.try_into_obj_not_found_in_pack_index_err()?;
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
        raw_kind: BitPackObjRawDeltified,
        base_offset: u64,
    ) -> BitResult<BitPackObjRaw> {
        trace!("expand_raw_obj(raw_kind: {:?}, base_offset: {})", raw_kind, base_offset);
        let (base, delta_bytes) = match raw_kind {
            BitPackObjRawDeltified::Raw(raw) => return Ok(raw),
            BitPackObjRawDeltified::Ofs(offset, delta) =>
                (self.read_obj_raw_at(base_offset - offset)?, delta),
            BitPackObjRawDeltified::Ref(base_oid, delta) => (self.read_obj_raw(base_oid)?, delta),
        };

        trace!("expand_raw_obj:base={:?}; delta_len={}", base, delta_bytes.len());
        base.expand_with_delta_bytes(&delta_bytes)
    }

    /// returns fully expanded raw object at offset
    pub fn read_obj_raw_at(&mut self, offset: u64) -> BitResult<BitPackObjRaw> {
        trace!("read_obj_raw_at(offset: {})", offset);
        match self.pack_obj_cache.get(&offset) {
            Some(raw) => Ok(raw.clone()),
            None => {
                let raw = self.pack_reader().read_obj_from_offset_raw(offset)?;
                let expanded = self.expand_raw_obj(raw, offset)?;
                self.pack_obj_cache.insert(offset, expanded.clone());
                Ok(expanded)
            }
        }
    }

    /// returns fully expanded raw object with oid
    pub fn read_obj_raw(&mut self, oid: Oid) -> BitResult<BitPackObjRaw> {
        trace!("read_obj_raw(oid: {})", oid);
        let offset = self.obj_offset(oid)?;
        trace!("read_obj_raw(oid: {}): found object at offset `{}`)", oid, offset);
        let raw = self.read_obj_raw_at(offset)?;
        Ok(raw)
    }

    pub fn read_obj_header(&mut self, oid: Oid) -> BitResult<BitObjHeader> {
        let (crc, offset) = self.obj_crc_offset(oid)?;
        trace!("read_obj_header(oid: {}); crc={}; offset={}", oid, crc, offset);
        let header = self.read_obj_header_at(offset)?;
        Ok(header)
    }

    fn read_obj_header_at(&mut self, offset: u64) -> BitResult<BitObjHeader> {
        trace!("read_obj_header_at(offset: {})", offset);
        let reader = self.pack_reader();
        let header = reader.read_header_from_offset(offset)?;
        // can we assume base_header definitely has same type?
        let base_header = match header.obj_type {
            BitPackObjType::Commit
            | BitPackObjType::Tree
            | BitPackObjType::Blob
            | BitPackObjType::Tag => return Ok(header.into()),
            BitPackObjType::OfsDelta => {
                let ofs = reader.read_offset()?;
                self.read_obj_header_at(offset - ofs)
            }
            BitPackObjType::RefDelta => {
                let oid = self.pack_reader().read_oid()?;
                self.read_obj_header(oid)
            }
        }?;
        Ok(BitObjHeader { size: header.size, obj_type: base_header.obj_type })
    }
}

#[allow(unused)]
#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct PackIndex {
    /// layer 1 of the fanout table
    pub fanout: [u32; FANOUT_ENTRYC],
    pub oids: Vec<Oid>,
    pub crcs: Vec<u32>,
    pub offsets: Vec<u32>,
    pub pack_hash: SHA1Hash,
}

impl PackIndex {
    fn build_fanout(oids: &[Oid]) -> [u32; FANOUT_ENTRYC] {
        let mut fanout = [0; FANOUT_ENTRYC];
        for oid in oids {
            fanout[oid[0] as usize] += 1;
        }
        for i in 1..FANOUT_ENTRYC {
            fanout[i] += fanout[i - 1];
        }
        fanout
    }
}

pub struct PackIndexReader<R> {
    reader: R,
    fanout: [u32; FANOUT_ENTRYC],
    oid_cache: FxHashMap<u64, Vec<Oid>>,
    crc_offset_cache: FxHashMap<Oid, (u32, u64)>,
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
        Ok(Self {
            reader,
            fanout,
            n,
            oid_cache: Default::default(),
            crc_offset_cache: Default::default(),
        })
    }
}

impl<R: BufReadSeek> PackIndexReader<R> {
    /// returns the offset of the object with oid `oid` in the packfile
    pub fn find_oid_crc_offset(&mut self, oid: Oid) -> BitResult<(u32, u64)> {
        match self.crc_offset_cache.get(&oid) {
            Some(&crc_offset) => Ok(crc_offset),
            None => {
                let crc_offset = self.find_oid_crc_offset_inner(oid)?;
                self.crc_offset_cache.insert(oid, crc_offset);
                Ok(crc_offset)
            }
        }
        // the following is nicer as we can avoid calculating the hash twice
        // it's violating the borrow checker in it's current form though
        // match self.crc_offset_cache.entry(oid) {
        //     Entry::Occupied(entry) => Ok(*entry.get()),
        //     Entry::Vacant(entry) => self
        //         .find_oid_crc_offset_inner(oid)
        //         .map(|crc_offset| entry.insert(crc_offset))
        //         .copied(),
        // }
    }

    fn find_oid_crc_offset_inner(&mut self, oid: Oid) -> BitResult<(u32, u64)> {
        trace!("PackIndexReader::find_oid_crc_offset(oid: {})", oid);
        let index = self.find_oid_index(oid)?;
        debug_assert_eq!(oid, self.read_from(Layer::Oid, index)?);
        let crc = self.read_from::<u32>(Layer::Crc, index)?;
        let mut offset = self.read_from::<u32>(Layer::Ofs, index)? as u64;
        trace!("PackIndexReader::find_oid_crc_offset(..) -> ({}, {})", crc, offset);

        if offset > MAX_OFFSET {
            let ext_index = offset & MAX_OFFSET;
            offset = self.read_from(Layer::Ext, ext_index as u64)?;
        }

        Ok((crc, offset))
    }

    /// returns the offset of the start of the layer relative to the start of
    /// the pack index in bytes
    pub fn offset_of(&mut self, layer: Layer, index: u64) -> u64 {
        debug_assert!(layer < Layer::Ext);
        const SIZE: [u64; 4] = [OID_SIZE as u64, CRC_SIZE, OFFSET_SIZE, EXT_OFFSET_SIZE];
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

        self.seek(SeekFrom::Start(PACK_IDX_HEADER_SIZE + FANOUT_SIZE + low * OID_SIZE as u64))?;

        let search = |oids: &[Oid]| match oids.binary_search(&oid) {
            Ok(idx) => Ok(low + idx as u64),
            Err(idx) => Err(anyhow!(BitError::ObjectNotFoundInPackIndex(oid, low + idx as u64))),
        };

        let hash = low.mk_fx_hash();
        match self.oid_cache.raw_entry_mut().from_key_hashed_nocheck(hash, &low) {
            RawEntryMut::Occupied(entry) => search(entry.get()),
            RawEntryMut::Vacant(entry) => {
                let oids = self.reader.read_vec((high - low) as usize).unwrap();
                search(entry.insert_hashed_nocheck(hash, low, oids).1)
            }
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

impl Serialize for PackIndex {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let mut writer = BufWriter::new(HashWriter::new_sha1(writer));
        writer.write_u32(PACK_IDX_MAGIC)?;
        writer.write_u32(2)?;
        writer.write_iter(&self.fanout)?;
        writer.write_iter(&self.oids)?;
        writer.write_iter(&self.crcs)?;
        writer.write_iter(&self.offsets)?;
        writer.write_oid(self.pack_hash)?;

        match writer.into_inner() {
            Ok(writer) => writer.write_hash()?,
            Err(..) => bail!("hash writer flush failed while writing pack index"),
        };
        Ok(())
    }
}

impl Deserialize for PackIndex {
    fn deserialize(reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut r = HashReader::new_sha1(reader);
        Self::parse_header(&mut r)?;
        let fanout = r.read_array::<u32, FANOUT_ENTRYC>()?;
        // the last value of the layer 1 fanout table is the number of
        // hashes we expect as it is cumulative
        let n = fanout[FANOUT_ENTRYC - 1] as usize;
        let oids = r.read_vec(n)?;
        debug_assert!(oids.is_sorted());

        let crcs = r.read_vec::<u32>(n)?;
        let offsets = r.read_vec::<u32>(n)?;

        // TODO 8-byte offsets for large packfiles
        // let big_offsets = todo!();
        let pack_hash = r.read_oid()?;
        let hash = r.finalize_sha1();
        let idx_hash = r.read_oid()?;

        ensure_eq!(idx_hash, hash);
        assert!(r.is_at_eof()?, "todo parse level 5 fanout for large indexes");
        Ok(Self { fanout, oids, crcs, offsets, pack_hash })
    }
}

impl PackIndex {
    fn parse_header(mut reader: impl BufRead) -> BitResult<()> {
        let magic = reader.read_u32()?;
        ensure_eq!(magic, PACK_IDX_MAGIC, "invalid pack index signature");
        let version = reader.read_u32()?;
        ensure_eq!(version, 2);
        Ok(())
    }
}

pub struct PackfileReader<R> {
    pub(crate) reader: R,
    objectc: u32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, FromPrimitive, ToPrimitive)]
enum BitPackObjType {
    Commit   = 1,
    Tree     = 2,
    Blob     = 3,
    Tag      = 4,
    OfsDelta = 6,
    RefDelta = 7,
}

impl BitPackObjType {
    pub fn try_from_u8(ty: u8) -> BitResult<Self> {
        BitPackObjType::from_u8(ty).ok_or_else(|| anyhow!("invalid bit pack object type"))
    }
}

impl From<BitPackObjType> for BitObjType {
    fn from(obj_type: BitPackObjType) -> BitObjType {
        match obj_type {
            BitPackObjType::Commit => BitObjType::Commit,
            BitPackObjType::Tree => BitObjType::Tree,
            BitPackObjType::Blob => BitObjType::Blob,
            BitPackObjType::Tag => BitObjType::Tag,
            BitPackObjType::OfsDelta | BitPackObjType::RefDelta => bug!("found delta object type"),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
struct BitPackObjHeader {
    obj_type: BitPackObjType,
    size: u64,
}

impl From<BitPackObjHeader> for BitObjHeader {
    fn from(header: BitPackObjHeader) -> BitObjHeader {
        let BitPackObjHeader { obj_type, size } = header;
        Self { obj_type: obj_type.into(), size }
    }
}

impl<R: BufRead> PackfileReader<R> {
    pub fn new(mut reader: R) -> BitResult<Self> {
        let objectc = Self::parse_header(&mut reader)?;
        Ok(Self { reader, objectc })
    }

    fn parse_header(mut reader: impl BufRead) -> BitResult<u32> {
        let sig = reader.read_array::<u8, 4>()?;
        ensure_eq!(&sig, PACK_SIGNATURE, "invalid packfile signature");
        let version = reader.read_u32()?;
        ensure_eq!(version, 2, "invalid packfile version `{}`", version);
        Ok(reader.read_u32()?)
    }

    // 3 bits object type
    // MSB is 1 then read next byte
    // the `size` here is the `size` shown in `git verify-pack` (not the `size-in-packfile`)
    // so the uncompressed size (i.e. we can call `take` on the zlib (decompressed) stream, rather than the compressed stream)
    // https://git-scm.com/docs/git-verify-pack
    #[inline]
    fn read_pack_obj_header(&mut self) -> BitResult<BitPackObjHeader> {
        let (ty, size) = self.read_le_varint_with_shift(3)?;
        let obj_type = BitPackObjType::try_from_u8(ty)?;
        Ok(BitPackObjHeader { obj_type, size })
    }

    fn inflate(&mut self, size: u64) -> BitResult<Vec<u8>> {
        let mut decompressor = Decompress::new(true);
        let mut output = Vec::with_capacity(size as usize);
        loop {
            let input = self.fill_buf()?;
            let at_eof = input.is_empty();
            let in_so_far = decompressor.total_in();
            let flush = if at_eof { FlushDecompress::Finish } else { FlushDecompress::None };
            let status = decompressor.decompress_vec(input, &mut output, flush)?;
            let consumed = decompressor.total_in() - in_so_far;
            self.consume(consumed as usize);
            match status {
                flate2::Status::Ok | flate2::Status::BufError => continue,
                flate2::Status::StreamEnd => break,
            }
        }
        assert_eq!(output.len() as u64, size);
        Ok(output)
    }

    fn read_pack_obj(&mut self) -> BitResult<BitPackObjRawDeltified> {
        let BitPackObjHeader { obj_type, size } = self.read_pack_obj_header()?;
        // the delta types have only the delta compressed but the size/baseoid is not,
        // the 4 base object types have all their data compressed
        // we so we have to treat them a bit differently
        let raw = match obj_type {
            BitPackObjType::Commit
            | BitPackObjType::Tree
            | BitPackObjType::Blob
            | BitPackObjType::Tag => BitPackObjRawDeltified::Raw(BitPackObjRaw {
                obj_type: BitObjType::from(obj_type),
                bytes: self.inflate(size)?,
            }),
            BitPackObjType::OfsDelta =>
                BitPackObjRawDeltified::Ofs(self.read_offset()?, self.inflate(size)?),
            BitPackObjType::RefDelta =>
                BitPackObjRawDeltified::Ref(self.read_oid()?, self.inflate(size)?),
        };

        Ok(raw)
    }

    /// Runs the closure `f` and returns the output of the closure along with the crc of the bytes consumed during it
    fn with_crc32<T>(
        &mut self,
        f: impl FnOnce(&mut PackfileReader<HashReader<Crc32, R>>) -> BitResult<T>,
    ) -> BitResult<(u32, T)> {
        let mut out = MaybeUninit::uninit();
        let mut crc = 0;
        let objectc = self.objectc;
        take_mut::take(&mut self.reader, |reader| {
            let reader: HashReader<Crc32, R> = HashReader::new_crc32(reader);
            let mut this: PackfileReader<HashReader<Crc32, R>> = PackfileReader { reader, objectc };
            out = MaybeUninit::new(f(&mut this));
            crc = this.reader.finalize_crc();
            this.reader.into_inner()
        });
        // SAFETY: out has now been initialized within the take_mut closure
        let out = unsafe { out.assume_init() };
        Ok((crc, out?))
    }

    /// Read the pack object also calculating the crc32 of the compressed data
    fn read_pack_obj_with_crc(&mut self) -> BitResult<(u32, BitPackObjRawDeltified)> {
        self.with_crc32(|this| this.read_pack_obj())
    }
}

impl<R: BufReadSeek> PackfileReader<R> {
    /// seek to `offset` and read pack object header
    #[inline]
    fn read_header_from_offset(&mut self, offset: u64) -> BitResult<BitPackObjHeader> {
        self.seek(SeekFrom::Start(offset))?;
        self.read_pack_obj_header()
    }

    pub fn read_obj_from_offset_raw(&mut self, offset: u64) -> BitResult<BitPackObjRawDeltified> {
        trace!("read_obj_from_offset_raw(offset: {})", offset);
        self.seek(SeekFrom::Start(offset))?;
        self.read_pack_obj()
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
