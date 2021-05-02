use super::*;
use crate::error::BitGenericError;
use crate::serialize::Deserialize;
use crate::time::Timespec;
use crate::tls;
use std::fmt::{self, Debug, Formatter};
use std::os::linux::fs::MetadataExt;

#[derive(Debug, PartialEq, Clone, Default)]
pub struct BitIndexEntries(BitIndexEntriesMap);

type BitIndexEntriesMap = BTreeMap<(BitPath, MergeStage), BitIndexEntry>;

impl Deref for BitIndexEntries {
    type Target = BitIndexEntriesMap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BitIndexEntries {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<BitIndexEntry>> for BitIndexEntries {
    fn from(entries: Vec<BitIndexEntry>) -> Self {
        Self(
            entries
                .into_iter()
                .map(|entry| ((entry.filepath, entry.flags.stage()), entry))
                .collect(),
        )
    }
}

impl Serialize for BitIndexEntry {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        assert!(self.filepath.is_relative());
        writer.write_timespec(self.ctime)?;
        writer.write_timespec(self.mtime)?;
        writer.write_u32(self.device)?;
        writer.write_u32(self.inode)?;
        writer.write_u32(self.mode.as_u32())?;
        writer.write_u32(self.uid)?;
        writer.write_u32(self.gid)?;
        writer.write_u32(self.filesize)?;
        writer.write_bit_hash(&self.hash)?;
        writer.write_u16(self.flags.0)?;
        writer.write_all(self.filepath.as_bytes())?;
        writer.write_all(&[0u8; 8][..self.padding_len()])?;
        Ok(())
    }
}

impl Deserialize for BitIndexEntry {
    fn deserialize(r: &mut dyn BufRead) -> BitResult<BitIndexEntry> {
        let ctime = r.read_timespec()?;
        let mtime = r.read_timespec()?;
        let device = r.read_u32()?;
        let inode = r.read_u32()?;
        let mode = FileMode::new(r.read_u32()?);
        let uid = r.read_u32()?;
        let gid = r.read_u32()?;
        let filesize = r.read_u32()?;
        let hash = r.read_bit_hash()?;
        let flags = BitIndexEntryFlags::new(r.read_u16()?);

        // read filepath until null terminator (inclusive)
        let mut filepath_bytes = vec![];
        r.read_until(0x00, &mut filepath_bytes)?;
        assert_eq!(*filepath_bytes.last().unwrap(), 0x00);
        let filepath = util::path_from_bytes(&filepath_bytes[..filepath_bytes.len() - 1]);

        assert!(filepath.is_relative());
        assert_eq!(flags.path_len() as usize, filepath.len());

        let entry = BitIndexEntry {
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
        };

        // read padding (to make entrysize multiple of 8)
        let mut padding = [0u8; 8];
        // we -1 from padding here as we have already read the
        // null byte belonging to the end of the filepath
        // this is safe as `0 < padding <= 8`
        r.read_exact(&mut padding[..entry.padding_len() - 1])?;
        assert_eq!(u64::from_be_bytes(padding), 0);

        Ok(entry)
    }
}

/// a representation of an individual entry into the indexfile
/// this also the uniform representation of tree (head) entries,
/// index entries, and workdir entries
/// and is the yielded type of `BitIterator`
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct BitIndexEntry {
    pub ctime: Timespec,
    pub mtime: Timespec,
    pub device: u32,
    pub inode: u32,
    pub mode: FileMode,
    pub uid: u32,
    /// group identifier of the current user
    pub gid: u32,
    pub filesize: u32,
    /// may be zero if left uncalculated (for efficiency)
    pub hash: BitHash,
    pub flags: BitIndexEntryFlags,
    //? is it necessary for this path to be relative to the repository workdir?
    pub filepath: BitPath,
}

impl From<TreeEntry> for BitIndexEntry {
    fn from(entry: TreeEntry) -> Self {
        // its fine to zero most of these fields as we know the hash, and that is what we
        // use to determine whether something has changed or not
        Self {
            ctime: Timespec::zero(),
            mtime: Timespec::zero(),
            device: 0,
            inode: 0,
            mode: entry.mode,
            uid: 0,
            gid: 0,
            filesize: 0,
            hash: entry.hash,
            flags: BitIndexEntryFlags::new(0),
            filepath: entry.path,
        }
    }
}

impl BitIndexEntry {
    pub fn as_key(&self) -> (BitPath, MergeStage) {
        (self.filepath, self.stage())
    }
}

const ENTRY_SIZE_WITHOUT_FILEPATH: usize = std::mem::size_of::<u64>() // ctime
            + std::mem::size_of::<u64>() // mtime
            + std::mem::size_of::<u32>() // device
            + std::mem::size_of::<u32>() // inode
            + std::mem::size_of::<u32>() // mode
            + std::mem::size_of::<u32>() // uid
            + std::mem::size_of::<u32>() // gid
            + std::mem::size_of::<u32>() // filesize
            + BIT_HASH_SIZE // hash
            + std::mem::size_of::<u16>(); // flags

impl TryFrom<BitPath> for BitIndexEntry {
    type Error = BitGenericError;

    fn try_from(path: BitPath) -> Result<Self, Self::Error> {
        let (canonical, relative) = tls::with_repo(|repo| {
            let canonical = repo.canonicalize(path)?;
            let relative = repo.to_relative_path(canonical)?;
            Ok((canonical, relative))
        })?;

        ensure!(!canonical.is_dir(), "bit index entry should not be a directory");
        let metadata = canonical.symlink_metadata().unwrap();

        // the path must be relative to the repository root
        // as this is the correct representation for the index entry
        // and otherwise, the pathlen in the flags will be off
        Ok(Self {
            filepath: relative,
            ctime: Timespec::ctime(&metadata),
            mtime: Timespec::mtime(&metadata),
            device: metadata.st_dev() as u32,
            inode: metadata.st_ino() as u32,
            mode: FileMode::from_metadata(&metadata),
            uid: metadata.st_uid(),
            gid: metadata.st_gid(),
            filesize: metadata.st_size() as u32,
            hash: BitHash::ZERO,
            flags: BitIndexEntryFlags::with_path_len(relative.len()),
        })
    }
}

impl BitIndexEntry {
    pub fn stage(&self) -> MergeStage {
        self.flags.stage()
    }

    pub(super) fn padding_len(&self) -> usize {
        Self::padding_len_for_filepath(self.filepath.len())
    }

    pub(super) fn padding_len_for_filepath(filepath_len: usize) -> usize {
        let entry_size = ENTRY_SIZE_WITHOUT_FILEPATH + filepath_len;
        // +8 instead of +7 as we should always have at least one byte
        // of padding as we consider the nullbyte of the filepath as padding
        let next_multiple_of_8 = ((entry_size + 8) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        assert!(padding_size > 0 && padding_size <= 8);
        padding_size
    }
}

impl PartialOrd for BitIndexEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitIndexEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.filepath.cmp(&other.filepath).then_with(|| self.stage().cmp(&other.stage()))
    }
}

/// 1  bit  assume-valid
/// 1  bit  extended
/// 2  bits stage
/// 12 bits name length if length is less than 0xFFF; otherwise store 0xFFF
// what is name really? probably path?
// probably doesn't really matter and is fine to just default flags to 0
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub struct BitIndexEntryFlags(u16);

impl Debug for BitIndexEntryFlags {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitIndexEntryFlags")
            .field("assume_valid", &self.assume_valid())
            .field("stage", &self.stage())
            .field("path_len", &self.path_len())
            .field("extended", &self.extended())
            .finish()
    }
}

impl BitIndexEntryFlags {
    pub fn with_path_len(len: usize) -> Self {
        Self(std::cmp::min(0xFFF, len as u16))
    }

    pub fn new(u: u16) -> Self {
        Self(u)
    }

    pub fn assume_valid(self) -> bool {
        self.0 & (1 << 15) != 0
    }

    pub fn extended(self) -> bool {
        self.0 & (1 << 14) != 0
    }

    pub fn stage(self) -> MergeStage {
        let stage = (self.0 & 0x3000) >> 12;
        MergeStage::try_from(stage as u8).unwrap()
    }

    pub fn path_len(self) -> u16 {
        self.0 & 0x0FFF
    }
}
