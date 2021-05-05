use crate::error::BitResult;
use crate::hash::{BitHash, SHA1Hash, BIT_HASH_SIZE};
use crate::io::{BufReadExt, HashReader, ReadExt};
use crate::serialize::{BufReadSeek, Deserialize};
use std::io::{BufRead, SeekFrom};

const PACK_IDX_MAGIC: u32 = 0xff744f63;
const FANOUT_ENTRYC: usize = 256;
const PACK_IDX_HEADER_SIZE: u64 = 8;

#[derive(Debug)]
pub struct PackIndex {
    /// layer 1 of the fanout table
    fanout: [u32; FANOUT_ENTRYC],
    hashes: Vec<BitHash>,
    crcs: Vec<u32>,
    offsets: Vec<u32>,
    pack_hash: SHA1Hash,
}

impl PackIndex {
    fn find_oid_index(mut r: &mut dyn BufReadSeek, oid: BitHash) -> BitResult<usize> {
        r.seek(SeekFrom::Start(PACK_IDX_HEADER_SIZE))?;
        let fanout = r.read_array::<u32, FANOUT_ENTRYC>()?;
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
        let low = if prefix == 0 { 0 } else { fanout[prefix - 1] } as i64;
        let high = fanout[prefix] as i64;

        r.seek(SeekFrom::Current(low * BIT_HASH_SIZE as i64))?;
        let oids = r.read_vec((high - low) as usize)?;
        match oids.binary_search(&oid) {
            Ok(idx) => Ok(low as usize + idx),
            Err(..) => Err(anyhow!("oid `{}` not found in packindex", oid)),
        }
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
