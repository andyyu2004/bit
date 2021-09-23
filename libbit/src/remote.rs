use crate::config::RemoteConfig;
use crate::error::{BitGenericError, BitResult};
use crate::path::BitPath;
use crate::repo::BitRepo;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

#[derive(Debug, PartialEq, Clone)]
pub struct Refspec {
    src: BitPath,
    dst: BitPath,
    forced: bool,
}

impl Refspec {
    pub fn default_fetch_for_remote(remote_name: &str) -> Self {
        let src = BitPath::intern("refs/heads/*");
        let dst = BitPath::intern(format!("refs/remotes/{}/*", remote_name));
        Self { src, dst, forced: true }
    }
}

impl FromStr for Refspec {
    type Err = BitGenericError;

    fn from_str(mut s: &str) -> BitResult<Self> {
        let forced = if &s[0..1] == "+" {
            s = &s[1..];
            true
        } else {
            false
        };
        let (src, dst) = s.split_once(':').ok_or_else(|| anyhow!("missing `:` in refspec"))?;
        Ok(Self { src: BitPath::intern(src), dst: BitPath::intern(dst), forced })
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

pub struct Remote {
    pub name: &'static str,
    pub config: RemoteConfig,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn add_remote(self, name: &str, url: &str) -> BitResult<()> {
        let refspec = Refspec::default_fetch_for_remote(name);
        self.with_raw_local_config(|config| {
            config.set_subsection("remote", name, "url", url)?;
            config.set_subsection("remote", name, "fetch", refspec)
        })?;

        Ok(())
    }

    pub fn ls_remotes(self) -> impl Iterator<Item = Remote> + 'rcx {
        self.remote_config().iter().map(|(name, config)| Remote { name, config: config.clone() })
    }
}

#[cfg(test)]
mod tests;
