use crate::BitResult;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

#[derive(PartialEq, Debug)]
pub struct Commit {
    tree: String,
    author: String,
    committer: String,
    message: String,
    gpgsig: Option<String>,
}

impl Commit {
    pub fn parse<R: Read>(r: R) -> BitResult<Self> {
        let r = BufReader::new(r);
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

            // everything after is part of the message
            if line.is_empty() {
                break;
            }

            let (k, v) = line.split_once(' ').unwrap();
            key = Some(k.to_owned());
            value = Some(v.to_owned());
        }

        let message = lines.collect::<Result<Vec<_>, _>>()?.join("\n");

        let tree = attrs["tree"].to_owned();
        let author = attrs["author"].to_owned();
        let committer = attrs["committer"].to_owned();
        let gpgsig = attrs.get("gpgsig").map(|sig| sig.to_owned());
        Ok(Self { tree, author, committer, message, gpgsig })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_commit() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitsingleline.commit");
        let commit = Commit::parse(bytes.as_slice())?;
        assert_eq!(&commit.tree, "d8329fc1cc938780ffdd9f94e0d364e0ea74f579");
        assert_eq!(&commit.author, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
        assert_eq!(&commit.committer, "Scott Chacon <schacon@gmail.com> 1243040974 -0700");
        assert_eq!(&commit.message, "First commit");
        Ok(())
    }

    #[test]
    fn parse_commit_with_multi_line_attr() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testcommitmultiline.commit");
        let commit = Commit::parse(bytes.as_slice())?;
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
}
