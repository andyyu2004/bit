use crate::error::BitResult;
use crate::hash::{BitHash, BIT_HASH_SIZE};
use crate::io_ext::{HashWriter, ReadExt, WriteExt};
use crate::obj::{FileMode, Tree, TreeEntry};
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::serialize::Serialize;
use crate::util;
use anyhow::anyhow;
use num_enum::TryFromPrimitive;
use sha1::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::{prelude::*, BufReader};
use std::iter::Peekable;
use std::ops::{Deref, DerefMut};
use std::path::Path;

// refer to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
#[derive(Debug, PartialEq, Clone, Default)]
pub struct BitIndex {
    pub header: BitIndexHeader,
    /// sorted by ascending by filepath (interpreted as unsigned bytes)
    /// ties broken by stage (a part of flags)
    // the link says `name` which usually refers to the hash
    // but it is sorted by filepath
    pub entries: BitIndexEntries,
    pub extensions: Vec<BitIndexExtension>,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct BitIndexEntries(BitIndexEntriesMap);
type BitIndexEntriesMap = BTreeMap<(BitPath, MergeStage), BitIndexEntry>;

impl BitIndex {
    fn create_tree(&self, _repo: &BitRepo) -> Tree {
        let entries = BTreeSet::new();
        Tree { entries }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = BitIndexEntry> + 'a {
        self.entries.values().copied()
    }
}

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

impl BitIndex {
    /// find entry by path
    pub fn find_entry(&self, path: BitPath, stage: MergeStage) -> Option<&BitIndexEntry> {
        self.entries.get(&(path, stage))
    }

    /// if entry with the same path already exists, it will be replaced
    pub fn add_entry(&mut self, entry: BitIndexEntry) {
        self.entries.insert((entry.filepath, entry.flags.stage()), entry);
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.keys().any(|(_, stage)| stage.is_merging())
    }

    /// builds a tree object from the current index entries
    pub fn build_tree(&self, repo: &BitRepo) -> BitResult<Tree> {
        if self.has_conflicts() {
            bail!("cannot write-tree an an index that is not fully merged");
        }
        TreeBuilder::new(self, repo, self.entries.values()).build()
    }
}

struct TreeBuilder<'a, I: Iterator<Item = &'a BitIndexEntry>> {
    index: &'a BitIndex,
    repo: &'a BitRepo,
    index_entries: Peekable<I>,
    // count the number of blobs created (not subtrees)
    // should match the number of index entries
}

impl<'a, I: Iterator<Item = &'a BitIndexEntry>> TreeBuilder<'a, I> {
    pub fn new(index: &'a BitIndex, repo: &'a BitRepo, entries: I) -> Self {
        Self { index, repo, index_entries: entries.peekable() }
    }

    fn build_tree(&mut self, current_index_dir: impl AsRef<Path>, depth: usize) -> BitResult<Tree> {
        let mut entries = BTreeSet::new();
        let current_index_dir = current_index_dir.as_ref();
        while let Some(next_entry) = self.index_entries.peek() {
            let &&BitIndexEntry { mode, filepath, hash, .. } = next_entry;
            // if the depth is greater than the number of components in the filepath
            // then we need to `break` and go out one level
            let (curr_dir, segment) = match filepath.try_split_path_at(depth) {
                Some(x) => x,
                None => break,
            };

            if curr_dir.as_path() != current_index_dir {
                break;
            }

            let nxt_path = curr_dir.as_path().join(segment);
            if nxt_path == filepath.as_path() {
                // only keep the final segment of the path inside the tree entry
                assert!(entries.insert(TreeEntry { mode, path: segment, hash }));
                self.index_entries.next();
            } else {
                let subtree = self.build_tree(&nxt_path, 1 + depth)?;
                let hash = self.repo.write_obj(&subtree)?;

                assert!(entries.insert(TreeEntry { path: segment, mode: FileMode::DIR, hash }));
            }
        }
        Ok(Tree { entries })
    }

    pub fn build(mut self) -> BitResult<Tree> {
        self.build_tree("", 0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BitIndexHeader {
    signature: [u8; 4],
    version: u32,
    entryc: u32,
}

impl Default for BitIndexHeader {
    fn default() -> Self {
        Self { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 0 }
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

    fn padding_len(&self) -> usize {
        Self::padding_len_for_filepath(self.filepath.len())
    }

    fn padding_len_for_filepath(filepath_len: usize) -> usize {
        let entry_size = ENTRY_SIZE_WITHOUT_FILEPATH + filepath_len;
        // +8 instead of +7 as we should always have at least one byte
        // of padding as we consider the nullbyte of the filepath as padding
        let next_multiple_of_8 = ((entry_size + 8) / 8) * 8;
        let padding_size = next_multiple_of_8 - entry_size;
        assert!(padding_size > 0 && padding_size <= 8);
        padding_size
    }
}

#[cfg(test)]
mod padding_tests {
    use super::*;

    #[test]
    fn index_entry_padding_test() {
        // dbg!(ENTRY_SIZE_WITHOUT_FILEPATH) = 62 atm;
        assert_eq!(BitIndexEntry::padding_len_for_filepath(8), 2);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(9), 1);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(10), 8);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(11), 7);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(12), 6);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(13), 5);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(14), 4);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(15), 3);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(16), 2);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(17), 1);
        assert_eq!(BitIndexEntry::padding_len_for_filepath(18), 8);
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

// this should be an enum of the concrete extensions
// but I don't really care about the extensions currently
// and they are optional anyway
#[derive(Debug, PartialEq, Clone)]
pub struct BitIndexExtension {
    pub signature: [u8; 4],
    pub size: u32,
    pub data: Vec<u8>,
}

// could probably do with better variant names once I know whats going on
#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, TryFromPrimitive, Copy, Clone)]
#[repr(u8)]
pub enum MergeStage {
    /// not merging
    None   = 0,
    Stage1 = 1,
    Stage2 = 2,
    Stage3 = 3,
}

impl MergeStage {
    pub fn is_merging(self) -> bool {
        self as u8 > 0
    }
}

impl Display for MergeStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl BitIndexEntryFlags {
    pub fn stage(self) -> MergeStage {
        let stage = 0x3000 & self.0;
        MergeStage::try_from(stage as u8).unwrap()
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

impl BitIndex {
    fn parse_header<R: BufRead>(r: &mut R) -> BitResult<BitIndexHeader> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        assert_eq!(&signature, b"DIRC");
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
        let flags = BitIndexEntryFlags(r.read_u16()?);

        // read filepath until null terminator (inclusive)
        let mut filepath_bytes = vec![];
        r.read_until(0x00, &mut filepath_bytes)?;
        assert_eq!(*filepath_bytes.last().unwrap(), 0x00);
        let filepath = util::path_from_bytes(&filepath_bytes[..filepath_bytes.len() - 1]);

        let entry = BitIndexEntry {
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

    fn parse_extensions(mut buf: &[u8]) -> BitResult<Vec<BitIndexExtension>> {
        let mut extensions = vec![];
        while buf.len() > BIT_HASH_SIZE {
            let signature: [u8; 4] = buf[0..4].try_into().unwrap();
            let size = u32::from_be_bytes(buf[4..8].try_into().unwrap());
            let data = buf[8..8 + size as usize].to_vec();
            extensions.push(BitIndexExtension { signature, size, data });
            buf = &buf[8 + size as usize..];
        }
        Ok(extensions)
    }

    // this impl currently is not ideal as it basically has to read it twice
    // although the second time is reading from memory so maybe its not that bad?
    pub fn deserialize<R: Read>(mut r: R) -> BitResult<BitIndex> {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut r = BufReader::new(&buf[..]);
        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| Self::parse_index_entry(&mut r))
            .collect::<Result<Vec<BitIndexEntry>, _>>()?
            .into();

        let mut remainder = vec![];
        assert!(r.read_to_end(&mut remainder)? >= BIT_HASH_SIZE);
        let extensions = Self::parse_extensions(&remainder)?;

        let bit_index = Self { header, entries, extensions };

        let (bytes, hash) = buf.split_at(buf.len() - BIT_HASH_SIZE);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = BitHash::from(hasher.finalize());
        let expected_hash = BitHash::new(hash.try_into().unwrap());
        assert_eq!(actual_hash, expected_hash);

        Ok(bit_index)
    }
}

impl Serialize for BitIndexHeader {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let Self { signature, version, entryc } = self;
        writer.write(signature)?;
        writer.write(&version.to_be_bytes())?;
        writer.write(&entryc.to_be_bytes())?;
        Ok(())
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

impl Serialize for BitIndexExtension {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        writer.write_all(&self.signature)?;
        writer.write_u32(self.size)?;
        writer.write_all(&self.data)?;
        Ok(())
    }
}

impl Serialize for BitIndex {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let mut hash_writer = HashWriter::new_sha1(writer);
        self.header.serialize(&mut hash_writer)?;

        for entry in self.entries.values() {
            entry.serialize(&mut hash_writer)?;
        }

        for extension in &self.extensions {
            extension.serialize(&mut hash_writer)?;
        }

        let hash = BitHash::from(hash_writer.finalize_hash());
        hash_writer.write_bit_hash(&hash)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;
    use crate::path::BitPath;
    use std::io::BufReader;
    use std::str::FromStr;

    #[test]
    fn parse_large_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/largeindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        assert_eq!(index.entries.len(), 31);
        Ok(())
    }

    #[test]
    fn parse_and_serialize_small_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/smallindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        let mut buf = vec![];
        index.serialize(&mut buf)?;
        assert_eq!(bytes, buf);
        Ok(())
    }

    #[test]
    fn parse_and_serialize_large_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/largeindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        let mut buf = vec![];
        index.serialize(&mut buf)?;
        assert_eq!(bytes, buf);
        Ok(())
    }

    #[test]
    fn parse_small_index() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/smallindex") as &[u8];
        let index = BitIndex::deserialize(bytes)?;
        // data from `git ls-files --stage --debug`
        // the flags show up as  `1` under git, not sure how they're parsed exactly
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
                flags: BitIndexEntryFlags(12),
                filepath: BitPath::intern("dir/test.txt"),
                mode: FileMode::REG,
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
                flags: BitIndexEntryFlags(8),
                filepath: BitPath::intern("test.txt"),
                mode: FileMode::REG,
                hash: BitHash::from_str("ce013625030ba8dba906f756967f9e9ca394464a").unwrap(),
            },
        ]
        .into();

        let expected_index = BitIndex {
            header: BitIndexHeader { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 2 },
            entries,
            extensions: vec![],
        };

        assert_eq!(expected_index, index);
        Ok(())
    }

    #[test]
    fn parse_index_header() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/largeindex") as &[u8];
        let header = BitIndex::parse_header(&mut BufReader::new(bytes))?;
        assert_eq!(
            header,
            BitIndexHeader { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 0x1f }
        );
        Ok(())
    }

    /// ├── dir
    /// │  └── test.txt
    /// ├── dir2
    /// │  ├── dir2.txt
    /// │  └── nested
    /// │     └── coolfile.txt
    /// ├── test.txt
    /// └── zs
    ///    └── one.txt
    // tests some correctness properties of the tree generated from the index
    // reminder that the path of the tree entries should be relative to its immediate parent
    // TODO be nice to have some way to quickcheck some of this
    #[test]
    fn bit_index_write_tree_test() -> BitResult<()> {
        BitRepo::find("tests/repos/indextest", |repo| {
            let tree = repo.with_index(|index| index.build_tree(repo))?;
            let entries = tree.entries.into_iter().collect_vec();
            assert_eq!(entries[0].path, "dir");
            assert_eq!(entries[0].mode, FileMode::DIR);
            assert_eq!(entries[1].path, "dir2");
            assert_eq!(entries[1].mode, FileMode::DIR);
            assert_eq!(entries[2].path, "test.txt");
            assert_eq!(entries[2].mode, FileMode::REG);
            assert_eq!(entries[3].path, "zs");
            assert_eq!(entries[3].mode, FileMode::DIR);

            let dir2_tree = repo.read_obj(entries[1].hash)?.as_tree();
            let dir2_tree_entries = dir2_tree.entries.into_iter().collect_vec();
            assert_eq!(dir2_tree_entries[0].path, "dir2.txt");
            assert_eq!(dir2_tree_entries[1].path, "nested");

            let mut nested_tree = repo.read_obj(dir2_tree_entries[1].hash)?.as_tree();
            let coolfile_entry = nested_tree.entries.pop_first().unwrap();
            assert!(nested_tree.entries.is_empty());
            assert_eq!(coolfile_entry.path, "coolfile.txt");

            let coolfile_blob = repo.read_obj(coolfile_entry.hash)?.as_blob();
            assert_eq!(coolfile_blob.bytes, b"coolfile contents!");

            let test_txt_blob = repo.read_obj(entries[2].hash)?.as_blob();
            assert_eq!(test_txt_blob.bytes, b"hello\n");
            Ok(())
        })
    }
}
