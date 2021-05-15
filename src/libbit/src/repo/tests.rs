use super::*;
use crate::cmd::BitHashObjectOpts;
use crate::obj;

impl BitRepo {
    /// be careful when deleting `rm foo` as the symlink points at it
    pub fn with_sample_repo<R>(f: impl FnOnce(&Self) -> BitResult<R>) -> BitResult<R> {
        Self::with_test_repo(|repo| {
            touch!(repo: "foo");
            touch!(repo: "bar");
            mkdir!(repo: "dir");
            mkdir!(repo: "dir/bar");
            touch!(repo: "dir/baz");
            touch!(repo: "dir/bar.l");
            touch!(repo: "dir/bar/qux");
            symlink!(repo: "bar" <- "dir/link");

            bit_add_all!(repo);
            bit_commit!(repo);
            f(repo)
        })
    }

    // sample repository with a series of commits
    // can't precompute commit hashes as the time is always changing
    pub fn with_sample_repo_commits<R>(
        f: impl FnOnce(&Self, Vec<Oid>) -> BitResult<R>,
    ) -> BitResult<R> {
        let strs = ["a", "b", "c", "d", "e"];
        let mut commit_oids = Vec::with_capacity(strs.len());
        Self::with_test_repo(|repo| {
            touch!(repo: "foo");
            for s in &strs {
                modify!(repo: "foo" << s);
                bit_add!(repo: "foo");
                commit_oids.push(bit_commit!(repo));
            }

            f(repo, commit_oids)
        })
    }
}

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
fn test_read_symlink_reads_contents_unresolved() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        modify!(repo: "foo" < "test content");
        assert_eq!(cat!(repo: "foo"), "test content");
        let hash = repo.hash_blob("dir/link".into())?;

        let symlink_hash = hash_symlink!(repo: "dir/link");
        assert_eq!(symlink_hash, hash);
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
