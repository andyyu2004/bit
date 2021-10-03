use super::*;
use crate::remote::Remote;

// We have to reenter the repository so we see the refreshed configuration
// The current configuration is not updated on change for now
#[test]
fn test_add_remote() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        assert!(repo.ls_remotes().next().is_none());
        repo.add_remote("foo", "bar")?;
        let mut remotes = repo.ls_remotes();
        assert_eq!(
            remotes.next().unwrap(),
            Remote {
                name: "foo",
                fetch: Refspec::default_fetch_for_remote("foo"),
                url: GitUrl::parse("bar")?,
            }
        );
        assert!(remotes.next().is_none());
        Ok(())
    })
}

#[test]
fn test_remove_non_existent_remote() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        assert!(repo.ls_remotes().next().is_none());
        repo.remove_remote("nonexistent").unwrap_err();
        Ok(())
    })
}

#[test_env_log::test]
fn test_remove_remote() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        assert!(repo.ls_remotes().next().is_none());
        repo.add_remote("foo", "bar")?;
        repo.remove_remote("foo")?;
        let mut remotes = repo.ls_remotes();
        assert!(remotes.next().is_none());
        Ok(())
    })
}

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
                    url: GitUrl::parse("git@github.com:andyyu2004/bit")?,
                    fetch: "+refs/heads/*:refs/remotes/origin/*".parse()?,
                },
                "gitlab" => RemoteConfig {
                    url: GitUrl::parse("git@gitlab.com:andyyu2004/bit")?,
                    fetch: "+refs/heads/*:refs/remotes/origin/*".parse()?,
                }
            }
        }
    );
    Ok(())
}
