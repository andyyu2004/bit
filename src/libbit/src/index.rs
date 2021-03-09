use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::FileMode;
use crate::read_ext::ReadExt;
use crate::util;
use sha1::Digest;
use smallvec::{smallvec, SmallVec};
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;

// refer, version: (), entryc: ()  version: (), entryc: () version: (), entryc: ()to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
#[derive(Debug, PartialEq)]
struct BitIndex {
    header: BitIndexHeader,
    /// sorted by ascending by hash (interpreted as unsigned bytes)
    /// ties broken by stage (part of flags)
    entries: Vec<BitIndexEntry>,
}

#[derive(Debug, PartialEq)]
struct BitIndexHeader {
    signature: [u8; 4],
    version: u32,
    entryc: u32,
}

#[derive(Debug, PartialEq)]
struct BitIndexEntry {
    /// 32 secs followed by 32 nano
    ctime: u64,
    mtime: u64,
    device: u32,
    inode: u32,
    mode: FileMode,
    uid: u32,
    /// group identifier of the current user
    gid: u32,
    filesize: u32,
    hash: BitHash,
    // TODO probably deserves its own struct
    flags: u16,
    filepath: PathBuf,
}

impl BitIndex {
    fn parse_header<R: BufRead>(r: &mut R) -> BitResult<BitIndexHeader> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        let version = r.read_u32()?;
        assert_eq!(version, 2, "Only index format v2 is supported");
        let entryc = r.read_u32()?;

        Ok(BitIndexHeader { signature, version, entryc })
    }

    fn parse_index_entry<R: BufRead>(r: &mut R) -> BitResult<BitIndexEntry> {
        let ctime = r.read_u64()?;
        let mtime = r.read_u64()?;
        let device = r.read_u32()?;
        let inode = r.read_u32()?;
        let mode = FileMode::new(r.read_u32()?);
        let uid = r.read_u32()?;
        let gid = r.read_u32()?;
        let filesize = r.read_u32()?;
        let hash = r.read_bit_hash()?;
        let flags = r.read_u16()?;

        const BYTES_SO_FAR: usize = std::mem::size_of::<u64>() // ctime
            + std::mem::size_of::<u64>() // mtime
            + std::mem::size_of::<u32>() // device
            + std::mem::size_of::<u32>() // inode
            + std::mem::size_of::<u32>() // mode
            + std::mem::size_of::<u32>() // uid
            + std::mem::size_of::<u32>() // gid
            + std::mem::size_of::<u32>() // filsize
            + std::mem::size_of::<[u8; 20]>() // hash
            + std::mem::size_of::<u16>(); // flags

        // read filepath until null terminator (inclusive)
        let mut filepath_bytes = vec![];
        r.read_until(0x00, &mut filepath_bytes)?;

        // read padding (to make entrysize multiple of 8)
        let entry_size = BYTES_SO_FAR + filepath_bytes.len();
        let next_multiple_of_8 = ((entry_size + 7) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        assert!(padding_size < 8, "index entry padding was {}", padding_size);
        let mut void: SmallVec<[u8; 7]> = smallvec![0u8; padding_size];
        r.read_exact(&mut void)?;
        let filepath = util::path_from_bytes(&filepath_bytes[..filepath_bytes.len() - 1]);

        Ok(BitIndexEntry {
            ctime,
            mtime,
            device,
            inode,
            mode,
            uid,
            gid,
            filesize,
            hash,
            flags,
            filepath,
        })
    }

    fn deserialize<R: Read>(r: R) -> BitResult<BitIndex> {
        Self::deserialize_buffered(&mut BufReader::new(r))
    }

    fn deserialize_buffered<R: BufRead>(r: &mut R) -> BitResult<BitIndex> {
        let header = Self::parse_header(r)?;
        let entries = (0..header.entryc)
            .map(|_| Self::parse_index_entry(r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?;
        let hash = r.read_bit_hash()?;

        // TODO verify checksum
        Ok(Self { header, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn parse_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/index") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        Ok(())
    }

    #[test]
    fn parse_index_header() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/index") as &[u8];
        let header = BitIndex::parse_header(&mut BufReader::new(bytes))?;
        assert_eq!(
            header,
            BitIndexHeader { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 0x1f }
        );
        Ok(())
    }
}
