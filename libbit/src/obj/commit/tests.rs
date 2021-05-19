use super::*;
use crate::refs::{BitRef, SymbolicRef};
use crate::test_utils::*;
use quickcheck::{Arbitrary, Gen};

impl Arbitrary for Commit {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            tree: Arbitrary::arbitrary(g),
            parent: Arbitrary::arbitrary(g),
            author: Arbitrary::arbitrary(g),
            committer: Arbitrary::arbitrary(g),
            gpgsig: Some(generate_sane_string(100..300)),
            message: generate_sane_string(1..300),
        }
    }
}

#[test]
fn test_new_commit_moves_branch_not_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "somefile");
        bit_commit_all!(repo);

        // check HEAD has not moved
        assert_eq!(
            repo.head_ref().partially_resolve(repo)?,
            BitRef::Symbolic(SymbolicRef::branch("master"))
        );
        Ok(())
    })
}

#[test]
fn parse_commit() -> BitResult<()> {
    let bytes = include_bytes!("../../../tests/files/testcommitsingleline.commit") as &[u8];
    let commit = Commit::deserialize_from_slice(bytes)?;
    assert_eq!(hex::encode(commit.tree), "d8329fc1cc938780ffdd9f94e0d364e0ea74f579");
    // assert_eq!(&commit.author, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
    // assert_eq!(&commit.committer, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
    assert!(commit.gpgsig.is_none());
    assert_eq!(&commit.message, "First commit");
    Ok(())
}

#[test]
fn parse_commit_with_multi_line_attr() -> BitResult<()> {
    let bytes = include_bytes!("../../../tests/files/testcommitmultiline.commit");
    let commit = Commit::deserialize_from_slice(bytes.as_slice())?;
    let gpgsig = r#"-----BEGIN PGP SIGNATURE-----
iQIzBAABCAAdFiEExwXquOM8bWb4Q2zVGxM2FxoLkGQFAlsEjZQACgkQGxM2FxoL
kGQdcBAAqPP+ln4nGDd2gETXjvOpOxLzIMEw4A9gU6CzWzm+oB8mEIKyaH0UFIPh
rNUZ1j7/ZGFNeBDtT55LPdPIQw4KKlcf6kC8MPWP3qSu3xHqx12C5zyai2duFZUU
wqOt9iCFCscFQYqKs3xsHI+ncQb+PGjVZA8+jPw7nrPIkeSXQV2aZb1E68wa2YIL
3eYgTUKz34cB6tAq9YwHnZpyPx8UJCZGkshpJmgtZ3mCbtQaO17LoihnqPn4UOMr
V75R/7FjSuPLS8NaZF4wfi52btXMSxO/u7GuoJkzJscP3p4qtwe6Rl9dc1XC8P7k
NIbGZ5Yg5cEPcfmhgXFOhQZkD0yxcJqBUcoFpnp2vu5XJl2E5I/quIyVxUXi6O6c
/obspcvace4wy8uO0bdVhc4nJ+Rla4InVSJaUaBeiHTW8kReSFYyMmDCzLjGIu1q
doU61OM3Zv1ptsLu3gUE6GU27iWYj2RWN3e3HE4Sbd89IFwLXNdSuM0ifDLZk7AQ
WBhRhipCCgZhkj9g2NEk7jRVslti1NdN5zoQLaJNqSwO1MtxTmJ15Ksk3QP6kfLB
Q52UWybBzpaP9HEd4XnR+HuQ4k2K0ns2KgNImsNvIyFwbpMUyUWLMPimaV1DWUXo
5SBjDB/V/W2JBFR+XKHFJeFwYhj7DD/ocsGr4ZMx/lgc8rjIBkI=
=lgTX
-----END PGP SIGNATURE-----"#;
    assert_eq!(commit.gpgsig.as_deref(), Some(gpgsig));
    Ok(())
}

#[quickcheck_macros::quickcheck]
fn serialize_then_parse_commit(commit: Commit) -> BitResult<()> {
    let mut buf = vec![];
    commit.serialize(&mut buf)?;

    let parsed = Commit::deserialize_from_slice(buf.as_slice())?;
    assert_eq!(commit, parsed);
    Ok(())
}

#[test]
fn parse_commit_then_serialize_multiline() -> BitResult<()> {
    let bytes = include_bytes!("../../../tests/files/testcommitmultiline.commit");
    let commit = Commit::deserialize_from_slice(bytes.as_slice())?;

    let mut buf = vec![];
    commit.serialize(&mut buf)?;
    assert_eq!(bytes.as_slice(), &buf);
    Ok(())
}

#[test]
fn parse_commit_then_serialize_single_line() -> BitResult<()> {
    let bytes = include_bytes!("../../../tests/files/testcommitsingleline.commit");
    let commit = Commit::deserialize_from_slice(bytes.as_slice())?;

    println!("{}", commit);

    let mut buf = vec![];
    commit.serialize(&mut buf)?;
    assert_eq!(bytes.as_slice(), &buf);
    Ok(())
}
