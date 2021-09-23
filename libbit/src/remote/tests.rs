use super::*;

#[test]
fn test_parse_refspec() -> BitResult<()> {
    let refspec = "+refs/heads/master:refs/remotes/origin/master".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec {
            src: p!("refs/heads/master"),
            dst: p!("refs/remotes/origin/master"),
            forced: true
        }
    );

    let refspec = "refs/heads/master:refs/remotes/origin/master".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec {
            src: p!("refs/heads/master"),
            dst: p!("refs/remotes/origin/master"),
            forced: false
        }
    );

    let refspec = "+refs/heads/*:refs/remotes/origin/*".parse::<Refspec>()?;
    assert_eq!(
        refspec,
        Refspec { src: p!("refs/heads/*"), dst: p!("refs/remotes/origin/*"), forced: true }
    );

    Ok(())
}
