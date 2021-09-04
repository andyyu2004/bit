use crate::error::BitResult;
use crate::interner::Intern;
use crate::merge::ConflictStyle;
use crate::path::BitPath;
use crate::repo::BitRepo;
use git_config::file::GitConfig;
use git_config::values::{Boolean, Integer};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;

#[derive(Debug, Merge, Default)]
pub struct BitConfig {
    pub(crate) core: CoreConfig,
    pub(crate) user: UserConfig,
    pub(crate) merge: MergeConfig,
}

/// Defines a left biased merge operation
pub trait Merge {
    fn merge(&mut self, other: Self);
}

impl<T> Merge for Option<T> {
    fn merge(&mut self, other: Self) {
        if let None = self {
            *self = other
        }
    }
}

impl BitConfig {
    fn open(path: BitPath) -> BitResult<Self> {
        Self::from_gitconfig(&RawConfig::open(path)?)
    }

    fn from_gitconfig(config: &RawConfig<'_>) -> BitResult<Self> {
        Ok(Self {
            core: CoreConfig::from_config(config)?,
            user: UserConfig::from_config(config)?,
            merge: MergeConfig::from_config(config)?,
        })
    }

    /// Creates a merged configuration from the following sources (in order of increasing precedence):
    /// (NOT system atm /etc/gitconfig)
    /// - config dir (~/.config/git/config)
    /// - home directory (~/.gitconfig)
    /// - local config
    pub fn init(local_path: BitPath) -> BitResult<Self> {
        // start with the highest precedence config as `merge` is left-biased
        let mut config_paths = vec![local_path];

        if let Some(config_dir) = dirs::config_dir() {
            config_paths.push(BitPath::intern(config_dir.join("git/config")));
        }

        if let Some(home) = dirs::home_dir() {
            config_paths.push(BitPath::intern(home.join(".gitconfig")));
        }

        let mut config = BitConfig::default();
        for path in config_paths.into_iter().filter(|path| path.exists()) {
            config.merge(BitConfig::open(path)?);
        }
        Ok(config)
    }
}

#[derive(Debug, Merge, Default)]
pub struct MergeConfig {
    conflict_style: Option<ConflictStyle>,
}

impl MergeConfig {
    fn from_config(config: &RawConfig<'_>) -> BitResult<Self> {
        Ok(Self { conflict_style: config.get("merge", "conflictStyle")? })
    }
}

#[derive(Debug, Merge, Default)]
pub struct UserConfig {
    name: Option<String>,
    email: Option<String>,
}

impl UserConfig {
    fn from_config(config: &RawConfig<'_>) -> BitResult<Self> {
        Ok(Self { name: config.get("user", "name")?, email: config.get("user", "email")? })
    }
}

#[derive(Debug, Merge, Default)]
pub struct CoreConfig {
    repositoryformatversion: Option<i64>,
    bare: Option<bool>,
    filemode: Option<bool>,
    pager: Option<String>,
}

impl CoreConfig {
    fn from_config(config: &RawConfig<'_>) -> BitResult<Self> {
        macro_rules! get {
            ($key:literal ?? $default:expr) => {
                match config.get("core", $key)? {
                    Some(value) => value,
                    None => $default,
                }
            };
            ($key:literal) => {
                config.get("core", $key)?
            };
        }

        Ok(Self {
            repositoryformatversion: get!("repositoryformatversion"),
            bare: get!("bare"),
            filemode: get!("filemode"),
            pager: get!("pager"),
        })
    }
}

/// Wrapper around gitconfig with a higher level api
pub struct RawConfig<'c> {
    inner: GitConfig<'c>,
    path: BitPath,
}

impl<'rcx> BitRepo<'rcx> {
    /// Use this API for setting config values, otherwise use `.config()`
    /// The current repository config settings will NOT be refreshed
    pub fn with_raw_local_config(
        self,
        f: impl FnOnce(&mut RawConfig<'_>) -> BitResult<()>,
    ) -> BitResult<()> {
        debug_assert!(self.config_path().try_exists()?);
        let mut config = RawConfig::open(self.config_path())?;
        f(&mut config)?;
        config.write()?;
        Ok(())
    }
}

pub trait BitConfigValue: Sized {
    fn parse(s: &str) -> BitResult<Self>;
}

impl BitConfigValue for String {
    fn parse(s: &str) -> BitResult<Self> {
        Ok(s.to_owned())
    }
}

impl BitConfigValue for i64 {
    fn parse(s: &str) -> BitResult<Self> {
        let i = Integer::from_str(s).unwrap_or_else(|err| {
            panic!("failed to parse config value as integer `{}`: {}", s, err)
        });
        Ok(i.value << i.suffix.map(|suffix| suffix.bitwise_offset()).unwrap_or(0))
    }
}

impl BitConfigValue for bool {
    fn parse(s: &str) -> BitResult<Self> {
        let b = Boolean::try_from(s.to_owned()).unwrap_or_else(|err| {
            panic!("failed to parse config value as boolean `{}`: {}", s, err)
        });
        match b {
            Boolean::True(_) => Ok(true),
            Boolean::False(_) => Ok(false),
        }
    }
}

impl BitConfigValue for ConflictStyle {
    fn parse(s: &str) -> BitResult<Self> {
        match s {
            "merge" => Ok(ConflictStyle::Merge),
            "diff3" => Ok(ConflictStyle::Diff3),
            _ => bail!("unknown merge style `{}`", s),
        }
    }
}

impl<'c> RawConfig<'c> {
    pub fn open(path: BitPath) -> BitResult<Self> {
        Ok(Self { inner: GitConfig::open(path)?, path })
    }

    /// write the configuration to disk
    fn write(&self) -> BitResult<()> {
        let inner = &self.inner;
        let bytes: Vec<u8> = inner.into();
        let mut file = File::with_options().write(true).open(&self.path)?;
        file.write_all(&bytes)?;
        Ok(())
    }

    fn get_raw(&self, section: &str, key: &str) -> Option<Cow<'_, [u8]>> {
        self.inner.value(section, None, key).ok()
    }

    pub fn get<T: BitConfigValue>(&self, section: &str, key: &str) -> BitResult<Option<T>> {
        self.get_raw(section, key)
            .map(|bytes| T::parse(std::str::from_utf8(&bytes).expect("invalid utf8 in bitconfig")))
            .transpose()
    }

    /// Writes changes in memory but does not flush to disk
    pub fn set(&mut self, section_name: &str, key: &str, value: impl ToString) -> BitResult<()> {
        let mut section = match self.inner.section_mut(section_name, None) {
            Ok(section) => section,
            Err(_) => self.inner.new_section(section_name.intern(), None),
        };
        section.set(key.intern().into(), value.to_string().intern().as_bytes().into());
        Ok(())
    }
}

/// generates accessors for each property
/// searches up into global scope if the property is not found locally returning None
// if none of the configurations contain the value
macro_rules! get_opt {
    ($section:ident.$field:ident:$ty:ty) => {
        impl BitConfig {
            pub fn $field(&self) -> Option<$ty> {
                self.$section.$field.clone()
            }
        }
    };
}

macro_rules! get {
    ($section:ident.$field:ident:$ty:ty, $default:expr) => {
        impl BitConfig {
            pub fn $field(&self) -> $ty {
                self.$section.$field.clone().unwrap_or($default)
            }
        }
    };
}

get!(core.filemode: bool, false);
get!(core.pager: String, "less".to_owned());

get!(merge.conflict_style: ConflictStyle, ConflictStyle::Merge);

get_opt!(core.repositoryformatversion: i64);
get_opt!(core.bare: bool);
get_opt!(user.name: String);
get_opt!(user.email: String);
