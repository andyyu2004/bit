use super::*;
use crate::io::BufReadExt;
use crate::iter::BitEntry;
use crate::serialize::Deserialize;
use crate::time::Timespec;
use std::fmt::{self, Debug, Formatter};
use std::iter::FromIterator;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

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

impl FromIterator<BitIndexEntry> for BitIndexEntries {
    fn from_iter<T: IntoIterator<Item = BitIndexEntry>>(iter: T) -> Self {
        Self(iter.into_iter().map(|entry| (entry.key(), entry)).collect())
    }
}

impl Serialize for BitIndexEntry {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        assert!(self.path.is_relative());
        writer.write_timespec(self.ctime)?;
        writer.write_timespec(self.mtime)?;
        writer.write_u32(self.device)?;
        writer.write_u32(self.inode)?;
        writer.write_u32(self.mode.as_u32())?;
        writer.write_u32(self.uid)?;
        writer.write_u32(self.gid)?;
        writer.write_u32(self.filesize)?;
        writer.write_oid(self.oid)?;
        writer.write_u16(self.flags.0)?;
        if self.flags.extended() {
            writer.write_u16(self.extended_flags.0)?;
        }
        writer.write_all(self.path.as_bytes())?;
        writer.write_all(&[0u8; 8][..self.padding_len()])?;
        Ok(())
    }
}

impl Deserialize for BitIndexEntry {
    fn deserialize(mut r: impl BufRead) -> BitResult<BitIndexEntry> {
        let ctime = r.read_timespec()?;
        let mtime = r.read_timespec()?;
        let device = r.read_u32()?;
        let inode = r.read_u32()?;
        let mode = FileMode::new(r.read_u32()?);
        let uid = r.read_u32()?;
        let gid = r.read_u32()?;
        let filesize = r.read_u32()?;
        let oid = r.read_oid()?;
        let flags = BitIndexEntryFlags::new(r.read_u16()?);
        let extended_flags = BitIndexEntryExtendedFlags::new(
            flags.extended().then(|| r.read_u16()).transpose()?.unwrap_or_default(),
        );

        // optimization of skipping ahead flags.path_len() bytes instead of a linear scan to find the next null byte
        let path = r.read_null_terminated_path_skip_n(flags.path_len() as usize)?;

        debug_assert!(path.is_relative());
        debug_assert!(
            flags.path_len() as usize == path.len()
                || path.len() > 0xfff && flags.path_len() == 0xff
        );

        let entry = BitIndexEntry {
            ctime,
            mtime,
            device,
            inode,
            mode,
            uid,
            gid,
            filesize,
            oid,
            flags,
            extended_flags,
            path,
        };

        // read padding (to make entrysize multiple of 8)
        let mut padding = [0u8; 8];
        // we -1 from padding here as we have already read the
        // null byte belonging to the end of the filepath
        // this is safe as `0 < padding <= 8`
        r.read_exact(&mut padding[..entry.padding_len() - 1])?;
        debug_assert_eq!(u64::from_be_bytes(padding), 0);

        Ok(entry)
    }
}

/// a representation of an individual entry into the indexfile
/// this also the uniform representation of tree (head) entries,
/// index entries, and workdir entries
/// and is the yielded type of `BitIterator`
// NOTE: this type is rather large and so while it is copy out of necessity,
// we should probably try to pass it by reference where possible
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
    pub oid: Oid,
    pub flags: BitIndexEntryFlags,
    pub extended_flags: BitIndexEntryExtendedFlags,
    pub path: BitPath,
}

impl From<TreeEntry> for BitIndexEntry {
    fn from(entry: TreeEntry) -> Self {
        // its fine to zero most of these fields as we know the hash, and that is the only thing we
        // need to use to determine whether anything has changed
        Self {
            ctime: Timespec::ZERO,
            mtime: Timespec::ZERO,
            device: 0,
            inode: 0,
            mode: entry.mode,
            uid: 0,
            gid: 0,
            filesize: Self::UNKNOWN_SIZE,
            oid: entry.oid,
            flags: BitIndexEntryFlags::with_path_len(entry.path.len()),
            extended_flags: BitIndexEntryExtendedFlags::default(),
            path: entry.path,
        }
    }
}

impl BitIndexEntry {
    pub fn key(&self) -> (BitPath, MergeStage) {
        (self.path, self.stage())
    }
}

// also excludes extended flags
const ENTRY_SIZE_WITHOUT_FILEPATH: usize = std::mem::size_of::<u64>() // ctime
            + std::mem::size_of::<u64>() // mtime
            + std::mem::size_of::<u32>() // device
            + std::mem::size_of::<u32>() // inode
            + std::mem::size_of::<u32>() // mode
            + std::mem::size_of::<u32>() // uid
            + std::mem::size_of::<u32>() // gid
            + std::mem::size_of::<u32>() // filesize
            + OID_SIZE // hash
            + std::mem::size_of::<u16>(); // flags

impl BitIndexEntry {
    pub const UNKNOWN_SIZE: u32 = u32::MAX;

    pub fn from_path(repo: BitRepo<'_>, path: &Path) -> BitResult<Self> {
        let normalized = repo.normalize_path(path)?;
        let relative = repo.to_relative_path(&normalized)?;

        debug_assert!(!normalized.is_dir(), "bit index entry should not be a directory");
        let metadata = normalized.symlink_metadata()?;

        // the path must be relative to the repository root
        // as this is the correct representation for the index entry
        // and otherwise, the pathlen in the flags will be off
        let path = BitPath::intern(relative);
        Ok(Self {
            path,
            ctime: Timespec::ctime(&metadata),
            mtime: Timespec::mtime(&metadata),
            device: metadata.dev() as u32,
            inode: metadata.ino() as u32,
            mode: FileMode::from_metadata(&metadata),
            uid: metadata.uid(),
            gid: metadata.gid(),
            filesize: metadata.size() as u32,
            // we don't calculate oid upfront as an optimization
            // we first try to use the index metadata to detect change
            oid: Oid::UNKNOWN,
            flags: BitIndexEntryFlags::with_path_len(path.len()),
            extended_flags: BitIndexEntryExtendedFlags::default(),
        })
    }

    pub fn stage(&self) -> MergeStage {
        self.flags.stage()
    }

    pub fn set_stage(&mut self, stage: MergeStage) {
        self.flags.set_stage(stage)
    }

    pub fn is_unmerged(&self) -> bool {
        self.stage().is_unmerged()
    }

    pub(super) fn padding_len(&self) -> usize {
        Self::padding_len_for_filepath(self.path.len(), self.flags.extended())
    }

    pub(super) fn padding_len_for_filepath(filepath_len: usize, is_extended: bool) -> usize {
        let entry_size = ENTRY_SIZE_WITHOUT_FILEPATH
            + filepath_len
            + is_extended.then_some(std::mem::size_of::<BitIndexEntryExtendedFlags>()).unwrap_or(0);
        // +8 instead of +7 as we should always have at least one byte
        // of padding as we consider the nullbyte of the filepath as padding
        let next_multiple_of_8 = ((entry_size + 8) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        debug_assert!(padding_size > 0 && padding_size <= 8);
        padding_size
    }
}

impl BitEntry for BitIndexEntry {
    fn oid(&self) -> Oid {
        self.oid
    }

    fn path(&self) -> BitPath {
        self.path
    }

    fn mode(&self) -> FileMode {
        self.mode
    }
}

/// 1  bit  assume-valid
/// 1  bit  extended
/// 2  bits stage
/// 12 bits name length if length is less than 0xFFF; otherwise store 0xFFF (length excludes the null byte)
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
// TODO revisit this if a less random arby impl is required
#[cfg_attr(test, derive(BitArbitrary))]
#[repr(transparent)]
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

    pub fn set_stage(&mut self, stage: MergeStage) {
        // reset relevant bits to 0
        self.0 &= !0x3000;
        // and then set them again
        self.0 |= (stage as u16) << 12;
        assert_eq!(self.stage(), stage);
    }

    pub fn path_len(self) -> u16 {
        self.0 & 0x0FFF
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct BitIndexEntryExtendedFlags(u16);

impl BitIndexEntryExtendedFlags {
    pub fn new(flags: u16) -> Self {
        // bottom 13-bits unused, must be 0
        debug_assert!(flags & 0x1000 == 0);
        Self(flags)
    }
}
