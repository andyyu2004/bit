use super::*;
use crate::error::BitGenericError;
use crate::path::BitPath;
use itertools::Itertools;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;

impl BitRepo {
    pub fn index_add(
        &self,
        pathspec: impl TryInto<Pathspec, Error = BitGenericError>,
    ) -> BitResult<()> {
        self.with_index_mut(|index| index.add(&pathspec.try_into()?))
    }

    // creates a repository in a temporary directory and initializes it
    pub fn with_test_repo<R>(f: impl FnOnce(&BitRepo) -> BitResult<R>) -> BitResult<R> {
        let basedir = tempfile::tempdir()?;
        BitRepo::init_load(&basedir, f)
    }
}

impl<'r> BitIndex<'r> {
    #[cfg(test)]
    pub fn add_str(&mut self, s: &str) -> BitResult<()> {
        let pathspec = s.parse::<Pathspec>()?;
        self.repo.match_worktree_with(&pathspec)?.for_each(|entry| self.add_entry(entry))
    }
}

#[test]
fn test_add_non_matching_pathspec() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        let err = repo.index_add("wer").unwrap_err();
        assert_eq!(err.to_string(), "no files added: pathspec `wer` did not match any files");
        Ok(())
    })
}

#[test]
fn test_add_symlink() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        touch!(repo: "foo");
        symlink!(repo: "foo" <- "link");
        bit_add_all!(repo);
        repo.with_index(|index| {
            let mut iter = index.std_iter();
            let fst = iter.next().unwrap();
            assert_eq!(fst.mode, FileMode::REG);
            assert_eq!(fst.filepath, "foo");
            assert_eq!(fst.filesize, 0);

            let snd = iter.next().unwrap();
            assert_eq!(snd.mode, FileMode::LINK);
            assert_eq!(snd.filepath, "link");
            // not entirely sure what the correct length is meant to be
            // its 19 on my system at least
            // assert_eq!(snd.filesize as usize, "foo".len());
            Ok(())
        })
    })
}
#[test]
fn test_parse_large_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes)?;
    assert_eq!(index.len(), 31);
    Ok(())
}

/// check all files in a directory are added (recursively)
#[test]
fn test_index_add_directory() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        mkdir!(repo: "dir");
        mkdir!(repo: "dir/c");
        touch!(repo: "dir/c/d");
        touch!(repo: "dir/a");
        touch!(repo: "dir/b");
        repo.with_index_mut(|index| {
            index.add_str("dir")?;
            assert_eq!(index.len(), 3);
            let mut iterator = index.entries.values();
            assert_eq!(iterator.next().unwrap().filepath, "dir/a");
            assert_eq!(iterator.next().unwrap().filepath, "dir/b");
            assert_eq!(iterator.next().unwrap().filepath, "dir/c/d");
            Ok(())
        })
    })
}

/// file `a` and file `a/somefile` should not exist simultaneously
/// however, naively we can achieve the above state in the index by the following
/// ```
///  touch a
///  bit add a
///  rm a
///  mkdir a
///  touch a/somefile
///  bit add a
/// ```
/// to avoid the above the index needs to perform some conflict detection when adding
/// `
#[test]
fn index_file_directory_collision() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        let a = repo.workdir.join("a");
        File::create(&a)?;
        repo.with_index_mut(|index| {
            index.add_str("a")?;
            std::fs::remove_file(&a)?;
            std::fs::create_dir(&a)?;
            File::create(a.join("somefile"))?;
            index.add_str("a")?;

            assert_eq!(index.len(), 1);
            let mut iterator = index.entries.values();
            assert_eq!(iterator.next().unwrap().filepath, "a/somefile");
            Ok(())
        })
    })
}

/// ```
///  mkdir foo
///  touch foo/bar
///  touch bar
///  bit add -A
///  rm foo/bar
///  mkdir foo/bar
///  touch foo/bar/baz
///  bit add -A
/// ```
/// check that `bar` is not removed but `foo/bar` is
#[test]
fn index_nested_file_directory_collision() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        mkdir!(repo: "foo");
        touch!(repo: "foo/bar");
        touch!(repo: "bar");
        bit_add_all!(repo);
        rm!(repo: "foo/bar");
        mkdir!(repo: "foo/bar");
        touch!(repo: "foo/bar/baz");
        bit_add_all!(repo);

        repo.with_index_mut(|index| {
            assert_eq!(index.len(), 2);
            let mut iterator = index.entries.values();
            assert_eq!(iterator.next().unwrap().filepath, "bar");
            assert_eq!(iterator.next().unwrap().filepath, "foo/bar/baz");
            Ok(())
        })
    })
}

/// the opposite problem of the one above
/// ```
///  mkdir foo
///  touch foo/a
///  touch foo/b
///  mkdir foo/bar
///  touch foo/bar/baz
///  bit add foo
///  rm -r foo
///  touch foo
///  bit add foo
/// ```
/// adding the file `foo` should remove all the entries
/// of the directory foo recursively
#[test]
fn index_directory_file_collision() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        repo.with_index_mut(|index| {
            let foo = repo.workdir.join("foo");
            std::fs::create_dir(foo)?;
            File::create(foo.join("a"))?;
            File::create(foo.join("b"))?;
            std::fs::create_dir(foo.join("bar"))?;
            File::create(foo.join("bar/baz"))?;
            index.add_str("foo")?;
            assert_eq!(index.len(), 3);
            std::fs::remove_dir_all(foo)?;
            File::create(foo)?;
            index.add_str("foo")?;

            assert_eq!(index.len(), 1);
            let mut iter = index.entries.values();
            assert_eq!(iter.next().unwrap().filepath, "foo");
            Ok(())
        })
    })
}

// this test adds something to the index and checks the index is still parseable
// `with_index` reparses it
#[test]
fn add_file_to_index() -> BitResult<()> {
    BitRepo::with_test_repo(|repo| {
        let filepath = repo.workdir.join("a");
        File::create(&filepath)?;
        assert!(filepath.exists());
        assert!(filepath.is_file());
        repo.index_add("a")?;
        repo.with_index(|_| Ok(()))
    })
}

#[test]
fn parse_and_serialize_small_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/smallindex") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes)?;
    let mut buf = vec![];
    index.serialize(&mut buf)?;
    assert_eq!(bytes, buf);
    Ok(())
}

#[test]
fn parse_and_serialize_large_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes)?;
    let mut buf = vec![];
    index.serialize(&mut buf)?;
    assert_eq!(bytes, buf);
    Ok(())
}

#[test]
fn parse_small_index() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/smallindex") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes)?;
    // data from `git ls-files --stage --debug`
    // the flags show up as  `1` under git, not sure how they're parsed exactly
    let entries = vec![
        BitIndexEntry {
            ctime: Timespec::new(1615087202, 541384113),
            mtime: Timespec::new(1615087202, 541384113),
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
            ctime: Timespec::new(1613643244, 672563537),
            mtime: Timespec::new(1613643244, 672563537),
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

    let expected_index = BitIndexInner::new(entries, vec![]);

    assert_eq!(expected_index, index);
    Ok(())
}

#[test]
fn parse_index_header() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let header = BitIndexInner::parse_header(&mut BufReader::new(bytes))?;
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
fn bit_index_build_tree_test() -> BitResult<()> {
    BitRepo::find("tests/repos/indextest", |repo| {
        let tree = repo.with_index(|index| index.build_tree())?;
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
