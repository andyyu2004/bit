use super::*;
use crate::cmd::BitHashObjectOpts;
use crate::obj;

#[test]
fn repo_init_creates_correct_initial_local_config() -> BitResult<()> {
    let basedir = tempfile::tempdir()?;
    BitRepo::init_load(&basedir, |repo| {
        repo.with_local_config(|config| {
            assert_eq!(config.repositoryformatversion()?.unwrap(), 0);
            assert_eq!(config.bare()?.unwrap(), false);
            assert_eq!(config.filemode()?, true);
            Ok(())
        })
    })
}

#[test]
fn repo_relative_paths() -> BitResult<()> {
    let basedir = tempfile::tempdir()?;
    BitRepo::init_load(&basedir, |repo| {
        let joined = repo.relative_paths(&["path", "to", "dir"]);
        assert_eq!(joined, format!("{}/.git/path/to/dir", basedir.path().display()));
        Ok(())
    })
}

#[test]
fn init_on_file() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let filepath = dir.path().join("test");
    File::create(&filepath)?;
    let err = BitRepo::init(filepath).unwrap_err();
    assert!(err.to_string().contains("not a directory"));
    Ok(())
}

fn prop_bit_hash_object_cat_file_are_inverses_blob(bytes: Vec<u8>) -> BitResult<()> {
    let basedir = tempfile::tempdir()?;
    BitRepo::init_load(basedir.path(), |repo| {
        let file_path = basedir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        file.write_all(&bytes)?;

        let hash = repo.hash_object(BitHashObjectOpts {
            path: file_path,
            do_write: true,
            objtype: obj::BitObjType::Blob,
        })?;

        assert!(
            repo.relative_paths(&["objects", &hex::encode(&hash[0..1]), &hex::encode(&hash[1..])])
                .exists()
        );

        // this doesn't call `bit_cat_file` directly but this function is
        // basically all that it does internally
        let blob = repo.read_obj(hash)?.into_blob();

        assert_eq!(blob.bytes, bytes);
        Ok(())
    })
}

#[test]
fn test_hash_symlink() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let _ = repo.hash_blob("dir/link".into())?;
        Ok(())
    })
}

#[test]
fn test_bit_hash_object_cat_file_are_inverses_blob() -> BitResult<()> {
    prop_bit_hash_object_cat_file_are_inverses_blob(b"hello".to_vec())
}

#[quickcheck]
fn bit_hash_object_cat_file_are_inverses_blob(bytes: Vec<u8>) -> BitResult<()> {
    prop_bit_hash_object_cat_file_are_inverses_blob(bytes)
}
