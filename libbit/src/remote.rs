use crate::config::RemoteConfig;
use crate::error::{BitGenericError, BitResult};
use crate::interner::Intern;
use crate::path::BitPath;
use crate::refs::SymbolicRef;
use crate::repo::BitRepo;
use crate::transport::{FileTransport, Transport};
use git_url_parse::{GitUrl, Scheme};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

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
        let dst = BitPath::intern(format!("refs/remotes/{}/", remote_name));
        Self { src, dst, forced: true, glob: true }
    }

    /// Matches given `source` to `self.src` and returns the expanded destination if it matches
    pub fn match_ref(&self, source: SymbolicRef) -> Option<BitPath> {
        if self.glob {
            let suffix = source.path().as_str().strip_prefix(self.src.as_str())?;
            Some(BitPath::intern(format!("{}{}", self.dst, suffix)))
        } else {
            if source.path() == self.src { Some(self.dst) } else { None }
        }
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
        write!(f, "{}:{}", self.src, self.dst)
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

impl<'rcx> BitRepo<'rcx> {
    pub fn add_remote(self, name: &str, url: &str) -> BitResult<()> {
        let refspec = Refspec::default_fetch_for_remote(name);
        self.with_raw_local_config(|config| {
            ensure!(!config.subsection_exists("remote", name), "remote `{}` already exists", name);
            config.set_subsection("remote", name, "url", url)?;
            config.set_subsection("remote", name, "fetch", refspec)
        })?;

        Ok(())
    }

    pub fn remove_remote(self, name: &str) -> BitResult<()> {
        if !self.with_raw_local_config(|config| Ok(config.remove_subsection("remote", name)))? {
            bail!("remote `{}` does not exist", name)
        };

        Ok(())
    }

    pub fn get_remote(self, name: &str) -> BitResult<Remote> {
        self.remote_config()
            .get(name)
            .map(|config| Remote::from_config(name.intern(), config.clone()))
            .ok_or_else(|| anyhow!("remote `{}` does not exist", name))
    }

    pub async fn fetch(self, name: &str) -> BitResult<()> {
        let remote = self.get_remote(name)?;
        self.fetch_remote(remote).await
    }

    pub async fn fetch_remote(self, remote: Remote) -> BitResult<()> {
        match remote.url.scheme {
            Scheme::Ssh => todo!("todo ssh"),
            Scheme::File => FileTransport::new(&remote)?.fetch(self).await,
            Scheme::Https => todo!("todo https"),
            Scheme::Unspecified => todo!("unspecified url scheme for remote"),
            _ => bail!("unsupported scheme `{}`", remote.url.scheme),
        }
    }

    pub fn ls_remotes(self) -> impl Iterator<Item = Remote> + 'rcx {
        self.remote_config().iter().map(|(name, config)| Remote::from_config(name, config.clone()))
    }
}

#[cfg(test)]
mod tests;
