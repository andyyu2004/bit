//! this does deviate a bit from the actual git config format
//! certain things will need to be rewritten to be valid toml

use crate::error::BitResult;
use crate::repo::BitRepo;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

lazy_static::lazy_static! {
    static ref GLOBAL_CONFIG_PATH: PathBuf = dirs::config_dir().unwrap().join("bit/.bitconfig");
    static ref GLOBAL_CONFIG: BitConfig = {
        let path: &Path = &GLOBAL_CONFIG_PATH;
        if !path.exists() {
            // this won't write to disk or anything
            BitConfig::default()
        } else {
            BitConfig::parse(&path)
                .unwrap_or_else(|err| panic!("failed to parse global bitconfig in {}: {}", path.display() ,err))
        }
    };
}

#[derive(Debug, Copy, Clone)]
pub enum BitConfigScope {
    Global,
    Local,
}

// tricky to manipulate the rust struct representation of config
// given `section.key` as a string at runtime
// just reread the file and modify it on disk and refresh the config of the repo
impl BitRepo {
    pub fn get_config_as_toml(&self, scope: BitConfigScope) -> BitResult<toml::Value> {
        let config_file_path = self.get_config_path(scope);
        Ok(toml::Value::from_str(&std::fs::read_to_string(config_file_path)?)?)
    }

    fn get_config_path(&self, scope: BitConfigScope) -> &Path {
        match scope {
            BitConfigScope::Global => &GLOBAL_CONFIG_PATH,
            BitConfigScope::Local => self.config_path(),
        }
    }

    pub fn get_config(
        &self,
        scope: BitConfigScope,
        section: &str,
        key: &str,
    ) -> BitResult<Option<String>> {
        let config = self.get_config_as_toml(scope)?;
        Ok(Self::read_config(&config, section, key))
    }

    // a helper function to allow us to use ? to propogate options
    // to avoid nested and_then's
    fn read_config(config: &toml::Value, section: &str, key: &str) -> Option<String> {
        let section = config.get(section)?;
        let value_opt = section.get(key)?;
        let value = value_opt
            .as_str()
            .unwrap_or_else(|| panic!("expected string value for `{}.{}`", section, key))
            .to_owned();
        Some(value)
    }

    pub fn set_config(
        &self,
        scope: BitConfigScope,
        section: &str,
        key: &str,
        value: &str,
    ) -> BitResult<()> {
        let mut config = self.get_config_as_toml(scope)?;
        let section = config
            .as_table_mut()
            .unwrap()
            .entry(section)
            .or_insert_with(|| toml::value::Table::default().into());
        section.as_table_mut().unwrap().insert(key.to_owned(), value.into());
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(self.get_config_path(scope))?;
        write!(file, "{}", toml::to_string_pretty(&config).unwrap())?;
        // the loaded config in repo is NOT updated
        // the assumption is that setting config is the last thing that will be done
        // with this instance of the repo, and the next command will reload the repo and
        // hence get the updated configuration
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitConfig {
    #[serde(default = "BitCoreConfig::default")]
    core: BitCoreConfig,
    #[serde(default = "BitUserConfig::default")]
    user: BitUserConfig,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitCoreConfig {
    repositoryformatversion: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitUserConfig {
    name: Option<String>,
    email: Option<String>,
}

//* be careful to not recursively call the generated method on the global config
macro_rules! get_opt {
    ($section:ident.$field:ident:$ty:ty) => {
        impl BitConfig {
            #[inline]
            pub fn $field(&self) -> Option<$ty> {
                self.$section.$field.or_else(|| GLOBAL_CONFIG.$section.$field)
            }
        }
    };
}

macro_rules! get_opt_ref {
    ($section:ident.$field:ident:$ty:ty) => {
        impl BitConfig {
            #[inline]
            pub fn $field(&self) -> Option<$ty> {
                self.$section.$field.as_deref().or_else(|| GLOBAL_CONFIG.$section.$field.as_deref())
            }
        }
    };
}

get_opt!(core.repositoryformatversion: i32);
get_opt_ref!(user.name: &str);
get_opt_ref!(user.email: &str);

impl BitConfig {
    pub fn parse(path: impl AsRef<Path>) -> BitResult<Self> {
        if !path.as_ref().exists() {
            return Ok(Self::default());
        }
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
}
