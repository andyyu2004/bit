use super::*;
use crate::refs::{BitRef, SymbolicRef};
use crate::repo::BitRepo;
use crate::test_utils::*;
use quickcheck::{Arbitrary, Gen};
use rand::Rng;
use smallvec::SmallVec;

impl Arbitrary for MutableCommit {
    fn arbitrary(g: &mut Gen) -> Self {
        let arbitrary_parents = (0..rand::thread_rng().gen_range(0..5))
            .map(|_| Oid::arbitrary(g))
            .collect::<SmallVec<_>>();
        Self::new_with_gpg(
            Arbitrary::arbitrary(g),
            arbitrary_parents,
            Arbitrary::arbitrary(g),
            Arbitrary::arbitrary(g),
            Arbitrary::arbitrary(g),
            Some(generate_sane_string_with_newlines(100..300)),
        )
    }
}

impl Arbitrary for CommitMessage {
    fn arbitrary(_g: &mut Gen) -> Self {
        Self {
            subject: "\ncommit message subject".to_owned(),
            message: "\n\ncommit message content".to_owned(),
        }
    }
}

// it's a bit awkward to do the other way as the Arby impl for CommitMessage generates some invalid commits such as a subject that starts with \n\n
#[quickcheck]
fn test_parse_and_display_commit_message_quickcheck(s: String) -> BitResult<()> {
    if s.trim_start().is_empty() {
        return Ok(());
    }
    let msg = CommitMessage::from_str(&s)?;
    let t = msg.to_string();
    assert_eq!(s, t);
    Ok(())
}

#[test]
fn test_parse_commit_message_with_trailing_newline_in_message() {
    let message = CommitMessage {
        subject: "\ncommit message subject".to_owned(),
        message: "\n\ncommit message content\n\n".to_owned(),
    };

    let parsed = CommitMessage::from_str(&message.to_string()).unwrap();
    assert_eq!(message, parsed);
}

// doing a manual one too as quickcheck generates some pretty crazy strings
#[test]
fn test_parse_and_display_commit_message() -> BitResult<()> {
    for _ in 0..100 {
        let mut s = generate_sane_string_with_newlines(2..100);
        // just to avoid an empty subject
        s.insert(0, 'a');
        let msg = CommitMessage::from_str(&s)?;
        let t = msg.to_string();
        assert_eq!(s, t);
    }
    Ok(())
}

#[test]
fn test_new_commit_moves_branch_not_head() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        touch!(repo: "somefile");
        bit_commit_all!(repo);

        // check HEAD has not moved
        assert_eq!(
            repo.partially_resolve_ref(SymbolicRef::HEAD)?,
            BitRef::Symbolic(SymbolicRef::new_branch("master"))
        );
        Ok(())
    })
}

#[test]
fn parse_commit() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/testcommitsingleline.commit") as &[u8];
    let commit = MutableCommit::deserialize_from_slice(bytes)?;
    assert_eq!(hex::encode(commit.tree), "d8329fc1cc938780ffdd9f94e0d364e0ea74f579");
    // assert_eq!(&commit.author, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
    // assert_eq!(&commit.committer, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
    assert!(commit.gpgsig.is_none());
    assert_eq!(&commit.message.subject, "First commit");
    Ok(())
}

#[test]
fn parse_commit_with_multi_line_attr() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/testcommitmultiline.commit");
    let commit = MutableCommit::deserialize_from_slice(bytes.as_slice())?;
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
fn serialize_then_parse_commit(commit: MutableCommit) -> BitResult<()> {
    let mut buf = vec![];
    commit.serialize(&mut buf)?;

    let parsed = MutableCommit::deserialize_from_slice(buf.as_slice())?;
    assert_eq!(commit, parsed);
    Ok(())
}

#[test]
fn parse_commit_then_serialize_multiline() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/testcommitmultiline.commit");
    let commit = MutableCommit::deserialize_from_slice(bytes.as_slice())?;

    let mut buf = vec![];
    commit.serialize(&mut buf)?;
    assert_eq!(bytes.as_slice(), &buf);
    Ok(())
}

#[test]
fn parse_commit_then_serialize_single_line() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/testcommitsingleline.commit");
    let commit = MutableCommit::deserialize_from_slice(bytes.as_slice())?;

    let mut buf = vec![];
    commit.serialize(&mut buf)?;
    assert_eq!(bytes.as_slice(), &buf);
    Ok(())
}
