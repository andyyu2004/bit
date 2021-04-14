use itertools::Itertools;

use super::*;
use crate::path::BitPath;
use std::io::BufReader;
use std::str::FromStr;

#[test]
fn parse_large_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let index = BitIndex::deserialize(bytes)?;
    assert_eq!(index.entries.len(), 31);
    Ok(())
}

#[test]
fn parse_and_serialize_small_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/smallindex") as &[u8];
    let index = BitIndex::deserialize(bytes)?;
    let mut buf = vec![];
    index.serialize(&mut buf)?;
    assert_eq!(bytes, buf);
    Ok(())
}

#[test]
fn parse_and_serialize_large_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let index = BitIndex::deserialize(bytes)?;
    let mut buf = vec![];
    index.serialize(&mut buf)?;
    assert_eq!(bytes, buf);
    Ok(())
}

#[test]
fn parse_small_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/smallindex") as &[u8];
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
            flags: BitIndexEntryFlags::new(12),
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
            flags: BitIndexEntryFlags::new(8),
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
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
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
        assert_eq!(entries[2].path, "exec");
        assert_eq!(entries[2].mode, FileMode::EXEC);
        assert_eq!(entries[3].path, "test.txt");
        assert_eq!(entries[3].mode, FileMode::REG);
        assert_eq!(entries[4].path, "zs");
        assert_eq!(entries[4].mode, FileMode::DIR);

        let dir2_tree = repo.read_obj(entries[1].hash)?.into_tree();
        let dir2_tree_entries = dir2_tree.entries.into_iter().collect_vec();
        assert_eq!(dir2_tree_entries[0].path, "dir2.txt");
        assert_eq!(dir2_tree_entries[1].path, "nested");

        let mut nested_tree = repo.read_obj(dir2_tree_entries[1].hash)?.into_tree();
        let coolfile_entry = nested_tree.entries.pop_first().unwrap();
        assert!(nested_tree.entries.is_empty());
        assert_eq!(coolfile_entry.path, "coolfile.txt");

        let coolfile_blob = repo.read_obj(coolfile_entry.hash)?.into_blob();
        assert_eq!(coolfile_blob.bytes, b"coolfile contents!");

        let test_txt_blob = repo.read_obj(entries[3].hash)?.into_blob();
        assert_eq!(test_txt_blob.bytes, b"hello\n");
        Ok(())
    })
}

#[test]
fn test_bit_index_entry_flags() {
    let flags = BitIndexEntryFlags::new(0xb9fa);
    assert!(flags.assume_valid());
    assert!(!flags.extended());
    assert_eq!(flags.stage(), MergeStage::Stage3);
    assert_eq!(flags.path_len(), 0x9fa);
}

#[test]
fn index_flags_test() {
    // tests may look a bit dumb, but I'm bad at messing with bits
    assert_eq!(BitIndexEntryFlags::with_path_len(20).path_len(), 20);
    assert_eq!(BitIndexEntryFlags::with_path_len(0x1000).path_len(), 0xFFF);
}

#[test]
fn index_entry_padding_test() {
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
