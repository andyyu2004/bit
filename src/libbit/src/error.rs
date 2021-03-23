




pub type BitResult<T> = anyhow::Result<T>;
pub type BitError = anyhow::Error;

// #[derive(Debug, Error)]
// pub enum BitError {
//     #[error("{0}")]
//     IO(#[from] std::io::Error),
//     #[error("failed to parse toml: {0}")]
//     Toml(#[from] toml::de::Error),
//     #[error("`{0}` is not a directory")]
//     NotDirectory(PathBuf),
//     #[error("not a bit repository (or any of the parent directories)")]
//     BitDirectoryNotFound,
//     #[error("{0}")]
//     Msg(String),
//     #[error("{0}")]
//     StaticMsg(&'static str),
//     #[error("{0}")]
//     ParseIntError(#[from] ParseIntError),
//     #[error("index is not fully merged")]
//     Unmerged(),
//     #[error("bitconfig error: {0}")]
//     ConfigError(String),
//     #[error("aborting commit due to empty commit message")]
//     EmptyCommitMessage,
// }

// // don't really want a lifetime in BitError so we just display the string
// impl<'e> From<GitConfigError<'e>> for BitError {
//     fn from(err: GitConfigError<'e>) -> Self {
//         Self::ConfigError(err.to_string())
//     }
// }

// impl<'s> From<&'s str> for BitError {
//     fn from(s: &'s str) -> Self {
//         Self::Msg(s.to_owned())
//     }
// }

// impl From<String> for BitError {
//     fn from(s: String) -> Self {
//         Self::Msg(s)
//     }
// }
