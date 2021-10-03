use quickcheck::Arbitrary;

use super::*;

impl Arbitrary for Refspec {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            src: Arbitrary::arbitrary(g),
            dst: Arbitrary::arbitrary(g),
            forced: Arbitrary::arbitrary(g),
            glob: Arbitrary::arbitrary(g),
        }
    }
}

#[quickcheck]
fn test_serde_refspec(refspec: Refspec) -> BitResult<()> {
    assert_eq!(refspec.to_string().parse::<Refspec>()?, refspec);
    Ok(())
}

#[test]
fn test_parse_refspec() -> BitResult<()> {
    let refspec = "+refs/heads/master:refs/remotes/origin/master".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec {
            src: p!("refs/heads/master"),
            dst: p!("refs/remotes/origin/master"),
            forced: true,
            glob: false,
        }
    );

    let refspec = "refs/heads/master:refs/remotes/origin/master".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec {
            src: p!("refs/heads/master"),
            dst: p!("refs/remotes/origin/master"),
            forced: false,
            glob: false
        }
    );

    let refspec = "+refs/heads/*:refs/remotes/origin/*".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec {
            src: p!("refs/heads/"),
            dst: p!("refs/remotes/origin/"),
            forced: true,
            glob: true
        }
    );

    Ok(())
}

#[test]
fn test_match_refspec() -> BitResult<()> {
    let refspec = "+refs/heads/master:refs/remotes/origin/master".parse::<Refspec>()?;
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/master")),
        Some(symbolic!("refs/remotes/origin/master"))
    );

    assert_eq!(refspec.match_ref(symbolic!("refs/heads/other")), None,);

    let refspec = "+refs/heads/*:refs/remotes/origin/*".parse::<Refspec>()?;
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/master")),
        Some(symbolic!("refs/remotes/origin/master"))
    );
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/mybranch")),
        Some(symbolic!("refs/remotes/origin/mybranch"))
    );
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/local/mybranch")),
        Some(symbolic!("refs/remotes/origin/local/mybranch"))
    );
    assert_eq!(refspec.match_ref(symbolic!("refs/bad/master")), None,);
    Ok(())
}

#[test]
fn test_fetch() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let remote_path = repos_dir!("logic");
        repo.add_remote("origin", remote_path.to_str().unwrap())?;
        repo.fetch_blocking("origin")
    })
}
