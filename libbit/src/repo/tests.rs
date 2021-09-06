use super::*;
use crate::cmd::BitHashObjectOpts;

impl<'rcx> BitRepo<'rcx> {
    /// be careful when deleting `rm foo` as the symlink points at it
    /// WARNING: be very careful when changing this sample repo,
    /// it's probably better to create another repo based on this one
    /// as many tests depend on this
    /// WARNING: for some reason the symlink seems to change hashes every run so do not use this if you are testing hashes
    pub fn with_sample_repo<R>(f: impl FnOnce(BitRepo<'_>) -> BitResult<R>) -> BitResult<R> {
        Self::with_empty_repo(|repo| {
            touch!(repo: "foo");
            touch!(repo: "bar");
            mkdir!(repo: "dir");
            bit_commit_all!(repo);

            mkdir!(repo: "dir/bar");
            touch!(repo: "dir/baz");
            touch!(repo: "dir/bar.l");
            touch!(repo: "dir/bar/qux");
            symlink!(repo: "bar" <- "dir/link");

            bit_commit_all!(repo);
            f(repo)
        })
    }

    pub fn with_minimal_repo<R>(f: impl FnOnce(BitRepo<'_>) -> BitResult<R>) -> BitResult<R> {
        Self::with_empty_repo(|repo| {
            touch!(repo: "foo" < "default foo contents");
            bit_commit_all!(repo);
            f(repo)
        })
    }

    // same repo as above but without the symlink issue
    pub fn with_sample_repo_no_sym<R>(f: impl FnOnce(BitRepo<'_>) -> BitResult<R>) -> BitResult<R> {
        Self::with_empty_repo(|repo| {
            touch!(repo: "foo");
            touch!(repo: "bar");
            mkdir!(repo: "dir");
            bit_commit_all!(repo);

            mkdir!(repo: "dir/bar");
            touch!(repo: "dir/baz");
            touch!(repo: "dir/bar.l");
            touch!(repo: "dir/bar/qux");

            bit_commit_all!(repo);
            f(repo)
        })
    }

    // sample repository with a series of commits
    // can't precompute commit hashes as the time is always changing
    pub fn with_sample_repo_commits<R>(
        f: impl FnOnce(BitRepo<'_>, Vec<Oid>) -> BitResult<R>,
    ) -> BitResult<R> {
        let strs = ["a", "b", "c", "d", "e"];
        let mut commit_oids = Vec::with_capacity(strs.len());
        Self::with_empty_repo(|repo| {
            touch!(repo: "foo");
            for s in &strs {
                modify!(repo: "foo" << s);
                commit_oids.push(bit_commit_all!(repo).commit.oid());
            }

            f(repo, commit_oids)
        })
    }
}

#[test]
fn repo_checks_repo_for_version_zero() {
    let err = BitRepo::find(repos_dir!("notversion0"), |_repo| Ok(())).unwrap_err();
    assert_eq!(err.to_string(), "unsupported repositoryformatversion `2`, expected version 0");
}

#[test]
fn repo_init_creates_correct_initial_local_config() -> BitResult<()> {
    let basedir = tempfile::tempdir()?;
    BitRepo::init_load(&basedir, |repo| {
        let config = repo.config();
        assert_eq!(config.repositoryformatversion().unwrap(), 0);
        assert_eq!(config.bare().unwrap(), false);
        assert_eq!(config.filemode(), true);
        Ok(())
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
            objtype: BitObjType::Blob,
        })?;

        assert!(
            repo.relative_paths(&["objects", &hex::encode(&hash[0..1]), &hex::encode(&hash[1..])])
                .try_exists()?
        );

        // this doesn't call `bit_cat_file` directly but this function is
        // basically all that it does internally
        let blob = repo.read_obj(hash)?.into_blob();

        assert_eq!(blob.bytes(), bytes);
        Ok(())
    })
}

#[test]
fn test_read_symlink_reads_contents_unresolved() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        modify!(repo: "foo" < "test content");
        assert_eq!(cat!(repo: "foo"), "test content");
        let hash = repo.hash_blob_from_worktree("dir/link")?;

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
