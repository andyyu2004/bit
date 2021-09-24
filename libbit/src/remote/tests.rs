use super::*;

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
        Some(p!("refs/remotes/origin/master"))
    );

    assert_eq!(refspec.match_ref(symbolic!("refs/heads/other")), None,);

    let refspec = "+refs/heads/*:refs/remotes/origin/*".parse::<Refspec>()?;
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/master")),
        Some(p!("refs/remotes/origin/master"))
    );
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/mybranch")),
        Some(p!("refs/remotes/origin/mybranch"))
    );
    assert_eq!(
        refspec.match_ref(symbolic!("refs/heads/local/mybranch")),
        Some(p!("refs/remotes/origin/local/mybranch"))
    );
    assert_eq!(refspec.match_ref(symbolic!("refs/bad/master")), None,);
    Ok(())
}
