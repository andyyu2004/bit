use super::*;

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
        writer.write_u32(self.ctime_sec)?;
        writer.write_u32(self.ctime_nano)?;
        writer.write_u32(self.mtime_sec)?;
        writer.write_u32(self.mtime_nano)?;
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct BitIndexEntry {
    pub ctime_sec: u32,
    pub ctime_nano: u32,
    pub mtime_sec: u32,
    pub mtime_nano: u32,
    pub device: u32,
    pub inode: u32,
    pub mode: FileMode,
    pub uid: u32,
    /// group identifier of the current user
    pub gid: u32,
    pub filesize: u32,
    pub hash: BitHash,
    pub flags: BitIndexEntryFlags,
    pub filepath: BitPath,
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

/// 1  bit  assume-valid
/// 1  bit  extended
/// 2  bits stage
/// 12 bits name length if length is less than 0xFFF; otherwise store 0xFFF
// what is name really? probably path?
// probably doesn't really matter and is fine to just default flags to 0
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Default)]
pub struct BitIndexEntryFlags(u16);

impl BitIndexEntryFlags {
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

    pub fn name_length(self) -> u16 {
        self.0 & 0x0FFF
    }
}
