use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::{BitObj, BitObjType};
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

#[derive(PartialEq, Clone, Debug)]
pub struct Commit {
    tree: BitHash,
    parent: Option<BitHash>,
    author: String,
    committer: String,
    gpgsig: Option<String>,
    message: String,
}

impl Display for Commit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut buf = vec![];
        self.serialize(&mut buf).unwrap();
        write!(f, "{}", std::str::from_utf8(&buf).unwrap())
    }
}

impl BitObj for Commit {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        // adds the required spaces for multiline strings
        macro_rules! w {
            ($s:expr) => {
                writeln!(writer, "{}", $s.replace("\n", "\n "))
            };
        }

        w!(format!("tree {:}", self.tree))?;
        if let Some(parent) = &self.parent {
            w!(format!("parent {}", parent))?;
        }
        w!(format!("author {}", self.author))?;
        w!(format!("committer {}", self.committer))?;
        if let Some(gpgsig) = &self.gpgsig {
            w!(format!("gpgsig {}", gpgsig))?;
        }

        writeln!(writer)?;
        write!(writer, "{}", self.message)?;
        Ok(())
    }

    fn deserialize_buffered<R: BufRead>(r: &mut R) -> BitResult<Self> {
        let mut lines = r.lines();
        let mut attrs = HashMap::new();

        let mut key: Option<String> = None;
        let mut value: Option<String> = None;

        while let Some(line) = lines.next() {
            let line = line?;

            // line is a continuation of the previous line
            if let Some(v) = &mut value {
                if line.starts_with(' ') {
                    v.push('\n');
                    v.push_str(&line[1..]);
                    continue;
                } else {
                    attrs.insert(key.take().unwrap(), value.take().unwrap());
                }
            }

            // everything after the current (blank) line is part of the message
            if line.is_empty() {
                break;
            }

            let (k, v) =
                line.split_once(' ').unwrap_or_else(|| panic!("Failed to parse line `{}`", line));
            key = Some(k.to_owned());
            value = Some(v.to_owned());
        }

        let message = lines.collect::<Result<Vec<_>, _>>()?.join("\n");

        let tree = attrs["tree"].parse().unwrap();
        let parent = attrs.get("parent").map(|parent| parent.parse().unwrap());
        let author = attrs["author"].to_owned();
        let committer = attrs["committer"].to_owned();
        let gpgsig = attrs.get("gpgsig").map(|sig| sig.to_owned());
        Ok(Self { tree, parent, author, committer, message, gpgsig })
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Commit
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::*;
    use quickcheck::{Arbitrary, Gen};

    impl Arbitrary for Commit {
        fn arbitrary(g: &mut Gen) -> Self {
            fn mk_kv(_g: &mut Gen) -> String {
                format!("{} {}", generate_sane_string(1..30), generate_sane_string(1..100))
            }

            Self {
                tree: Arbitrary::arbitrary(g),
                parent: Arbitrary::arbitrary(g),
                author: generate_sane_string(0..100),
                committer: generate_sane_string(0..100),
                gpgsig: Some(generate_sane_string(100..300)),
                message: generate_sane_string(1..300),
            }
        }
    }

    #[test]
    fn parse_commit() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitsingleline.commit") as &[u8];
        let commit = Commit::deserialize(bytes)?;
        assert_eq!(hex::encode(commit.tree), "d8329fc1cc938780ffdd9f94e0d364e0ea74f579");
        assert_eq!(&commit.author, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
        assert_eq!(&commit.committer, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
        assert!(commit.gpgsig.is_none());
        assert_eq!(&commit.message, "First commit");
        Ok(())
    }

    #[test]
    fn parse_commit_with_multi_line_attr() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitmultiline.commit");
        let commit = Commit::deserialize(bytes.as_slice())?;
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

        let parsed = Commit::deserialize(buf.as_slice())?;
        assert_eq!(commit, parsed);
        Ok(())
    }

    #[test]
    fn parse_commit_then_serialize_multiline() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitmultiline.commit");
        let commit = Commit::deserialize(bytes.as_slice())?;

        let mut buf = vec![];
        commit.serialize(&mut buf)?;
        assert_eq!(bytes.as_slice(), &buf);
        Ok(())
    }

    #[test]
    fn parse_commit_then_serialize_single_line() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitsingleline.commit");
        let commit = Commit::deserialize(bytes.as_slice())?;

        let mut buf = vec![];
        commit.serialize(&mut buf)?;
        assert_eq!(bytes.as_slice(), &buf);
        Ok(())
    }
}
