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

struct HashReader<'a, D, R> {
    r: &'a mut R,
    hasher: D,
    bytes: Vec<u8>,
}

// bound by BufRead as this struct won't work properly without BufRead
impl<'a, R: BufRead, D: Digest> HashReader<'a, D, R> {
    pub fn new(r: &'a mut R) -> Self {
        Self { r, hasher: D::new(), bytes: vec![] }
    }
}

const EXPECTED_BYTES: [u8; 184] = [
    0x44, 0x49, 0x52, 0x43, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x60, 0x44, 0x46, 0x62,
    0x20, 0x44, 0xdd, 0xb1, 0x60, 0x44, 0x46, 0x62, 0x20, 0x44, 0xdd, 0xb1, 0x00, 0x01, 0x03, 0x06,
    0x00, 0x0e, 0xfb, 0xed, 0x00, 0x00, 0x81, 0xa4, 0x00, 0x00, 0x03, 0xe8, 0x00, 0x00, 0x03, 0xe8,
    0x00, 0x00, 0x00, 0x06, 0xce, 0x01, 0x36, 0x25, 0x03, 0x0b, 0xa8, 0xdb, 0xa9, 0x06, 0xf7, 0x56,
    0x96, 0x7f, 0x9e, 0x9c, 0xa3, 0x94, 0x46, 0x4a, 0x00, 0x0c, 0x64, 0x69, 0x72, 0x2f, 0x74, 0x65,
    0x73, 0x74, 0x2e, 0x74, 0x78, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x60, 0x2e, 0x3d, 0xec,
    0x28, 0x16, 0x81, 0x51, 0x60, 0x2e, 0x3d, 0xec, 0x28, 0x16, 0x81, 0x51, 0x00, 0x01, 0x03, 0x06,
    0x00, 0x0e, 0xc1, 0x1a, 0x00, 0x00, 0x81, 0xa4, 0x00, 0x00, 0x03, 0xe8, 0x00, 0x00, 0x03, 0xe8,
    0x00, 0x00, 0x00, 0x06, 0xce, 0x01, 0x36, 0x25, 0x03, 0x0b, 0xa8, 0xdb, 0xa9, 0x06, 0xf7, 0x56,
    0x96, 0x7f, 0x9e, 0x9c, 0xa3, 0x94, 0x46, 0x4a, 0x00, 0x08, 0x74, 0x65, 0x73, 0x74, 0x2e, 0x74,
    0x78, 0x74, 0x00, 0x00, 0x72, 0xcb, 0xf1, 0x53, 0x0d, 0x29, 0x40, 0xc3, 0xf6, 0xba, 0xd9, 0x7d,
    0xeb, 0x22, 0xd0, 0x71, 0x9d, 0x30, 0xa9, 0x39,
];

impl<'a, R: BufRead, D: Digest> Read for HashReader<'a, D, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.r.read(buf)?;
        // dbg!(&buf[..n]);
        let x = &buf[..n];
        dbg!(self.bytes.len());
        // assert_eq!(self.bytes, EXPECTED_BYTES[..self.bytes.len()]);
        self.bytes.extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

impl<'a, D: Digest, R: BufRead> BufRead for HashReader<'a, D, R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.r.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.r.consume(amt)
    }
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

        // read padding (to make entrysize multiple of 8)
        let entry_size = BYTES_SO_FAR + filepath_bytes.len();
        let next_multiple_of_8 = ((entry_size + 7) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        assert!(padding_size < 8, "index entry padding was {}", padding_size);
        // let mut void: SmallVec<[u8; 7]> = smallvec![0u8; padding_size];
        let mut void: Vec<u8> = vec![0u8; padding_size];
        r.read_exact(&mut void)?;
        let filepath = util::path_from_bytes(&filepath_bytes[..filepath_bytes.len() - 1]);

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

    fn deserialize<R: Read>(r: R) -> BitResult<BitIndex> {
        Self::deserialize_buffered(&mut BufReader::new(r))
    }

    fn deserialize_buffered<R: BufRead>(r: &mut R) -> BitResult<BitIndex> {
        let mut r = HashReader::<sha1::Sha1, _>::new(r);

        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| Self::parse_index_entry(&mut r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?;
        // TODO extensions?

        let hash = r.read_bit_hash()?;
        println!("{} bytes left in reader", r.read_to_end(&mut vec![])?);
        assert_eq!(r.read_to_end(&mut vec![])?, 0);
        let actual_hash = BitHash::new(r.hasher.finalize().try_into().unwrap());
        dbg!(hash);
        dbg!(actual_hash);

        // assert_eq!(r.bytes.len(), EXPECTED_BYTES.len());
        // assert_eq!(r.bytes, &EXPECTED_BYTES);

        // TODO verify checksum
        Ok(Self { header, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn parse_large_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/index") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        dbg!(&index);
        Ok(())
    }

    #[test]
    fn parse_small_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/smallindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        dbg!(&index);
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
