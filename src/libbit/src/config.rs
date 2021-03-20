use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitConfig {
    #[serde(default = "BitCoreConfig::default")]
    pub core: BitCoreConfig,
    #[serde(default = "BitUserConfig::default")]
    pub user: BitUserConfig,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitCoreConfig {
    pub repositoryformatversion: i32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BitUserConfig {
    pub name: Option<String>,
    pub email: Option<String>,
}
