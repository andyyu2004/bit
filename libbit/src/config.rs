//! this does deviate a bit from the actual git config format
//! certain things will need to be rewritten to be valid toml

// yes this file is pretty disgusting, but its only config right? :)
// I can't actually remember why its written the way it is, could consider a rewrite if something
// major requires changing

use crate::error::BitResult;
use crate::interner::Intern;
use crate::repo::BitRepo;
use git_config::file::GitConfig;
use git_config::values::{Boolean, Integer};
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

lazy_static! {
    static ref GLOBAL_PATH: PathBuf = dirs::home_dir().unwrap().join(".gitconfig");
}

#[derive(Debug, Copy, Clone)]
pub enum BitConfigScope {
    Global,
    Local,
}

pub struct BitConfig<'c> {
    inner: GitConfig<'c>,
    scope: BitConfigScope,
    path: PathBuf,
}

// this struct provides convenient access to each setting
// e.g. to access filemode, we can just write repo.config().filemode()
// its nicer to use than the with_config api
pub struct Config<'r> {
    repo: &'r BitRepo,
}

impl BitRepo {
    // this is only here to namespace all the configuration to not be directly under repo
    // although I do wonder if this is actually more annoying than helpful
    pub fn config(&self) -> Config<'_> {
        Config { repo: self }
    }

    pub fn with_config<R>(
        &self,
        scope: BitConfigScope,
        f: impl FnOnce(&mut BitConfig<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        match scope {
            BitConfigScope::Global => BitConfig::with_global_config(f),
            BitConfigScope::Local => self.with_local_config(f),
        }
    }

    pub fn with_local_config<R>(
        &self,
        f: impl for<'c> FnOnce(&mut BitConfig<'c>) -> BitResult<R>,
    ) -> BitResult<R> {
        BitConfig::with_local(self.config_path(), f)
    }
}

fn with_config<R>(
    scope: BitConfigScope,
    path: impl AsRef<Path>,
    f: impl for<'a> FnOnce(&mut BitConfig<'a>) -> BitResult<R>,
) -> BitResult<R> {
    let path = path.as_ref().to_path_buf();
    if !path.exists() {
        File::create(&path)?;
    }
    let s = std::fs::read_to_string(&path)?;
    let inner = GitConfig::try_from(s.as_str())
        .unwrap_or_else(|err| panic!("failed to parse bitconfig `{}`: {}", path.display(), err));

    let mut config = BitConfig { inner, path, scope };
    let ret = f(&mut config)?;
    Ok(ret)
}

impl<'c> BitConfig<'c> {
    /// write the configuration to disk
    fn write(&self) -> BitResult<()> {
        let inner = &self.inner;
        let bytes: Vec<u8> = inner.into();
        let mut file = File::with_options().write(true).open(&self.path)?;
        file.write_all(&bytes)?;
        Ok(())
    }

    pub fn with_local<R>(
        path: impl AsRef<Path>,
        f: impl FnOnce(&mut BitConfig<'_>) -> BitResult<R>,
    ) -> BitResult<R> {
        with_config(BitConfigScope::Local, path, f)
    }

    fn with_global_config<R>(f: impl FnOnce(&mut BitConfig<'_>) -> BitResult<R>) -> BitResult<R> {
        with_config(BitConfigScope::Global, GLOBAL_PATH.as_path(), f)
    }
}

pub trait BitConfigValue: Sized {
    fn get(bytes: &str) -> BitResult<Self>;
}

impl BitConfigValue for String {
    fn get(s: &str) -> BitResult<Self> {
        Ok(s.to_owned())
    }
}

impl BitConfigValue for i64 {
    fn get(s: &str) -> BitResult<Self> {
        let i = Integer::from_str(s).unwrap_or_else(|err| {
            panic!("failed to parse config value as integer `{}`: {}", s, err)
        });
        Ok(i.value << i.suffix.map(|suffix| suffix.bitwise_offset()).unwrap_or(0))
    }
}

impl BitConfigValue for bool {
    fn get(s: &str) -> BitResult<Self> {
        let b = Boolean::try_from(s.to_owned()).unwrap_or_else(|err| {
            panic!("failed to parse config value as boolean `{}`: {}", s, err)
        });
        match b {
            Boolean::True(_) => Ok(true),
            Boolean::False(_) => Ok(false),
        }
    }
}

impl<'c> BitConfig<'c> {
    fn get_raw(&self, section: &str, key: &str) -> Option<Cow<'_, [u8]>> {
        self.inner.value(section, None, key).ok()
    }

    pub fn get<T: BitConfigValue>(&self, section: &str, key: &str) -> BitResult<Option<T>> {
        self.get_raw(section, key)
            .map(|bytes| T::get(std::str::from_utf8(&bytes).expect("invalid utf8 in bitconfig")))
            .transpose()
    }

    pub fn set(&mut self, section_name: &str, key: &str, value: impl ToString) -> BitResult<()> {
        let mut section = match self.inner.section_mut(section_name, None) {
            Ok(section) => section,
            Err(_) => self.inner.new_section(section_name.intern(), None),
        };
        section.set(key.intern().into(), value.to_string().intern().as_bytes().into());
        self.write()?;
        Ok(())
    }
}

/// generates accessors for each property
/// searches up into global scope if the property is not found locally returning None
// if none of the configurations contain the value
macro_rules! get_opt {
    ($section:ident.$field:ident:$ty:ty) => {
        impl Config<'_> {
            pub fn $field(&self) -> BitResult<Option<$ty>> {
                self.repo.with_local_config(|config| config.$field())
            }
        }

        impl<'c> BitConfig<'c> {
            pub fn $field(&self) -> BitResult<Option<$ty>> {
                let section = stringify!($section);
                let field = stringify!($field);
                match self.get(section, field)? {
                    Some(value) => return Ok(Some(value)),
                    None => match self.scope {
                        BitConfigScope::Global => Ok(None),
                        BitConfigScope::Local => Self::with_global_config(|global| global.$field()),
                    },
                }
            }
        }
    };
}

macro_rules! get {
    ($section:ident.$field:ident:$ty:ty, $default:expr) => {
        impl Config<'_> {
            pub fn $field(&self) -> BitResult<$ty> {
                self.repo.with_local_config(|config| config.$field())
            }
        }

        impl<'c> BitConfig<'c> {
            pub fn $field(&self) -> BitResult<$ty> {
                let section = stringify!($section);
                let field = stringify!($field);
                match self.get(section, field)? {
                    Some(value) => return Ok(value),
                    None => match self.scope {
                        BitConfigScope::Global => Ok($default),
                        BitConfigScope::Local => Self::with_global_config(|global| global.$field()),
                    },
                }
            }
        }
    };
}

get!(core.filemode: bool, false);
get!(core.pager: String, "less".to_owned());

get_opt!(core.repositoryformatversion: i64);
get_opt!(core.bare: bool);
get_opt!(user.name: String);
get_opt!(user.email: String);
