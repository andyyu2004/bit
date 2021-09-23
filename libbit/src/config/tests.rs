use super::*;

#[test]
fn test_config_parse_remotes() -> BitResult<()> {
    let config = r#"
[core]
	repositoryformatversion = 0
	filemode = true
	bare = false
	logallrefupdates = true
[remote "origin"]
	url = git@github.com:andyyu2004/bit
	fetch = +refs/heads/*:refs/remotes/origin/*
[remote "gitlab"]
	url = git@gitlab.com:andyyu2004/bit
	fetch = +refs/heads/*:refs/remotes/origin/*
[branch "master"]
	remote = origin
	merge = refs/heads/master
    "#;

    let mut raw = RawConfig::new(config);
    let cfg = RemotesConfig::from_config(&mut raw)?;
    assert_eq!(
        cfg,
        RemotesConfig {
            remotes: hashmap! {
                "origin" => RemoteConfig {
                    url: "git@github.com:andyyu2004/bit",
                    fetch: "+refs/heads/*:refs/remotes/origin/*".parse()?,
                },
                "gitlab" => RemoteConfig {
                    url: "git@gitlab.com:andyyu2004/bit",
                    fetch: "+refs/heads/*:refs/remotes/origin/*".parse()?,
                }
            }
        }
    );
    Ok(())
}
