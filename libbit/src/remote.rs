use crate::config::RemoteConfig;
use crate::error::{BitGenericError, BitResult};
use crate::interner::Intern;
use crate::path::BitPath;
use crate::refs::{BitRef, SymbolicRef};
use crate::repo::BitRepo;
use crate::reset::ResetKind;
use crate::transport::{FileTransport, ProtocolTransport, SshTransport};
use git_url_parse::{GitUrl, Scheme};
use openssh::Session;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::str::FromStr;

pub const DEFAULT_REMOTE: &str = "origin";

#[derive(Debug, Clone)]
pub struct Refspec {
    /// The lhs of the `:` excluding the * if there is one
    src: BitPath,
    /// The rhs of the `:` excluding the * if there is one
    dst: BitPath,
    forced: bool,
    /// Whether both sides are globbed
    glob: bool,
}

impl PartialEq for Refspec {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src && self.dst == other.dst && self.forced == other.forced
    }
}

impl Refspec {
    pub fn default_fetch_for_remote(remote_name: &str) -> Self {
        let src = BitPath::intern("refs/heads/");
        let dst = BitPath::intern(format!("refs/remotes/{remote_name}/"));
        Self { src, dst, forced: true, glob: true }
    }

    fn reverse(&self) -> Self {
        let &Self { src, dst, forced, glob } = self;
        Self { src: dst, dst: src, forced, glob }
    }

    /// Matches given `dst` to `self.dst` and returns the expanded source if it matches
    pub fn reverse_match_ref(&self, dst: SymbolicRef) -> Option<SymbolicRef> {
        self.reverse().match_ref(dst)
    }

    /// Matches given `source` to `self.src` and returns the expanded destination if it matches
    pub fn match_ref(&self, source: SymbolicRef) -> Option<SymbolicRef> {
        let path = if self.glob {
            let suffix = source.path().as_str().strip_prefix(self.src.as_str())?;
            Some(BitPath::intern(format!("{}{}", self.dst, suffix)))
        } else {
            (source.path() == self.src).then_some(self.dst)
        }?;
        Some(SymbolicRef::new(path))
    }
}

impl FromStr for Refspec {
    type Err = BitGenericError;

    // very rough implementation, doesn't capture full semantics of refspecs
    fn from_str(mut s: &str) -> BitResult<Self> {
        let forced = if &s[0..1] == "+" {
            s = &s[1..];
            true
        } else {
            false
        };
        let (src, dst) = s.split_once(':').ok_or_else(|| anyhow!("missing `:` in refspec"))?;
        let (src, src_is_glob) = match src.strip_suffix('*') {
            Some(stripped) => (stripped, true),
            None => (src, false),
        };
        let (dst, dst_is_glob) = match dst.strip_suffix('*') {
            Some(stripped) => (stripped, true),
            None => (dst, false),
        };
        let glob = match (src_is_glob, dst_is_glob) {
            (true, true) => true,
            (false, false) => false,
            _ => bail!("only one side of refspec is globbed"),
        };
        Ok(Self { src: BitPath::intern(src), dst: BitPath::intern(dst), forced, glob })
    }
}

impl Display for Refspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.forced {
            write!(f, "+")?;
        }
        if self.glob {
            write!(f, "{}*:{}*", self.src, self.dst)
        } else {
            write!(f, "{}:{}", self.src, self.dst)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Remote {
    pub name: &'static str,
    pub url: GitUrl,
    pub fetch: Refspec,
}

impl Remote {
    fn from_config(name: &'static str, config: RemoteConfig) -> Self {
        Self { name, url: config.url, fetch: config.fetch }
    }
}

#[derive(Debug, PartialEq)]
pub enum FetchStatus {
    EmptyRemote,
    UpToDate,
    NotUpToDate,
}

#[derive(Debug, PartialEq)]
pub struct FetchSummary {
    /// The branch that HEAD is pointing to
    pub head_symref: Option<SymbolicRef>,
    pub status: FetchStatus,
}

impl FetchSummary {
    pub const EMPTY_REMOTE: Self = Self { head_symref: None, status: FetchStatus::EmptyRemote };
}

impl BitRepo {
    pub fn clone_blocking(into: impl AsRef<Path>, url: impl AsRef<str>) -> BitResult<()> {
        let into = into.as_ref();
        let exists = into.exists();
        if exists {
            ensure!(into.is_dir(), "file exists at clone path");
            ensure!(into.read_dir()?.next().is_none(), "cannot clone into non-empty directory");
        } else {
            std::fs::create_dir(into)?;
        }

        Self::init_load(into, |repo| {
            repo.add_remote(DEFAULT_REMOTE, url)?;
            repo.clone_origin_blocking()
        })
        .or_else(|err| {
            if !exists {
                std::fs::remove_dir_all(into)?;
            }
            Err(err)
        })
    }

    #[tokio::main]
    pub async fn clone_origin_blocking(&self) -> BitResult<()> {
        self.clone_origin().await
    }

    pub async fn clone_origin(&self) -> BitResult<()> {
        let remote = self.get_remote(DEFAULT_REMOTE)?;
        let FetchSummary { head_symref, status } = self.fetch_remote(&remote).await?;

        let refspec = remote.fetch;
        // TODO probably need to be a bit smarter than just defaulting to master
        let local = head_symref.unwrap_or(SymbolicRef::MASTER);
        dbg!(local);
        let remote = refspec.match_ref(local).expect(
            "todo this case where the branch remotes HEAD points to is not part of our refspec",
        );
        self.create_branch(local, BitRef::HEAD)?;

        if status == FetchStatus::EmptyRemote {
            return Ok(());
        }
        self.reset(remote, ResetKind::Hard)?;
        Ok(())
    }

    pub fn add_remote(&self, name: &str, url: impl AsRef<str>) -> BitResult<()> {
        let refspec = Refspec::default_fetch_for_remote(name);
        self.with_raw_local_config(|config| {
            ensure!(!config.subsection_exists("remote", name), "remote `{}` already exists", name);
            config.set_subsection("remote", name, "url", url.as_ref())?;
            config.set_subsection("remote", name, "fetch", refspec)?;
            Ok(())
        })
    }

    pub fn remove_remote(&self, name: &str) -> BitResult<()> {
        if !self.with_raw_local_config(|config| Ok(config.remove_subsection("remote", name)))? {
            bail!("remote `{}` does not exist", name)
        };
        Ok(())
    }

    pub fn get_remote(&self, name: &str) -> BitResult<Remote> {
        self.remote_config()
            .get(name)
            .map(|config| Remote::from_config(name.intern(), config.clone()))
            .ok_or_else(|| anyhow!("remote `{}` does not exist", name))
    }

    #[tokio::main]
    pub async fn fetch_blocking(&self, name: &str) -> BitResult<FetchSummary> {
        self.fetch(name).await
    }

    pub async fn fetch(&self, name: &str) -> BitResult<FetchSummary> {
        let remote = self.get_remote(name)?;
        self.fetch_remote(&remote).await
    }

    pub async fn fetch_remote(&self, remote: &Remote) -> BitResult<FetchSummary> {
        match remote.url.scheme {
            Scheme::Ssh => {
                let url = &remote.url;
                let dst = format!("{}@{}", url.user.as_ref().unwrap(), url.host.as_ref().unwrap());
                let session = Session::connect(dst, openssh::KnownHosts::Add).await?;
                let mut transport = SshTransport::new(self.clone(), &session, url).await?;
                transport.fetch(remote).await
            }
            Scheme::File => FileTransport::new(self, &remote.url).await?.fetch(remote).await,
            Scheme::Https => todo!("todo https"),
            Scheme::Unspecified => todo!("unspecified url scheme for remote"),
            _ => bail!("unsupported scheme `{}`", remote.url.scheme),
        }
    }

    pub fn ls_remotes(&self) -> impl Iterator<Item = Remote> {
        self.remote_config()
            .into_iter()
            .map(|(name, config)| Remote::from_config(name, config))
    }
}

#[cfg(test)]
mod clone_tests;
#[cfg(test)]
mod tests;
