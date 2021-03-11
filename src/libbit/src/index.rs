use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::FileMode;
use crate::read_ext::ReadExt;
use crate::util;
use sha1::Digest;
use smallvec::{smallvec, SmallVec};
use std::convert::TryInto;
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
    ctime_sec: u32,
    ctime_nano: u32,
    mtime_sec: u32,
    mtime_nano: u32,
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
        let ctime_sec = r.read_u32()?;
        let ctime_nano = r.read_u32()?;
        let mtime_sec = r.read_u32()?;
        let mtime_nano = r.read_u32()?;
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
        let filepath = util::path_from_bytes(&filepath_bytes[..filepath_bytes.len() - 1]);

        // read padding (to make entrysize multiple of 8)
        let entry_size = BYTES_SO_FAR + filepath_bytes.len();
        let next_multiple_of_8 = ((entry_size + 7) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        let mut padding = [0u8; 8];
        r.read_exact(&mut padding[..padding_size])?;
        assert_eq!(u64::from_be_bytes(padding), 0);

        Ok(BitIndexEntry {
            ctime_sec,
            ctime_nano,
            mtime_sec,
            mtime_nano,
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

    // this impl currently is not ideal as it basically has to read it twice
    // although the second time is reading from memory so maybe its not that bad?
    fn deserialize<R: Read>(mut r: R) -> BitResult<BitIndex> {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut r = BufReader::new(&buf[..]);
        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| Self::parse_index_entry(&mut r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?;

        let (bytes, hash) = buf.split_at(buf.len() - 20);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = BitHash::new(hasher.finalize().try_into().unwrap());
        let expected_hash = BitHash::new(hash.try_into().unwrap());
        assert_eq!(actual_hash, expected_hash);

        // TODO verify checksum
        Ok(Self { header, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::str::FromStr;

    #[test]
    fn parse_large_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/index") as &[u8];
        // this just checks it passes all the internal assertions
        let _index = BitIndex::deserialize(bytes)?;
        Ok(())
    }

    #[test]
    fn parse_small_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/smallindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        // data from `git ls-files --stage --debug`
        // the flags show up as  `0` under git, not sure how they're parsed exactly
        let entries = vec![
            BitIndexEntry {
                ctime_sec: 1615087202,
                ctime_nano: 541384113,
                mtime_sec: 1615087202,
                mtime_nano: 541384113,
                device: 66310,
                inode: 981997,
                uid: 1000,
                gid: 1000,
                filesize: 6,
                flags: 12,
                filepath: PathBuf::from("dir/test.txt"),
                mode: FileMode::READ_WRITE,
                hash: BitHash::from_str("ce013625030ba8dba906f756967f9e9ca394464a").unwrap(),
            },
            BitIndexEntry {
                ctime_sec: 1613643244,
                ctime_nano: 672563537,
                mtime_sec: 1613643244,
                mtime_nano: 672563537,
                device: 66310,
                inode: 966938,
                uid: 1000,
                gid: 1000,
                filesize: 6,
                flags: 8,
                filepath: PathBuf::from("test.txt"),
                mode: FileMode::READ_WRITE,
                hash: BitHash::from_str("ce013625030ba8dba906f756967f9e9ca394464a").unwrap(),
            },
        ];

        let expected_index = BitIndex {
            header: BitIndexHeader { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 2 },
            entries,
        };

        assert_eq!(expected_index, index);
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
