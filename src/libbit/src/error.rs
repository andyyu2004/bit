use std::num::ParseIntError;
use std::path::PathBuf;
use thiserror::Error;

pub type BitResult<T> = Result<T, BitError>;

#[derive(Debug, Error)]
pub enum BitError {
    #[error("{0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    Ini(#[from] ini::Error),
    #[error("`{0}` is not a directory")]
    NotDirectory(PathBuf),
    #[error("not a bit repository (or any of the parent directories)")]
    BitDirectoryNotFound,
    #[error("{0}")]
    Msg(String),
    #[error("{0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("index is not fully merged")]
    Unmerged(),
    #[error("aborting commit due to empty commit message")]
    EmptyCommitMessage,
}

impl<'s> From<&'s str> for BitError {
    fn from(s: &'s str) -> Self {
        Self::Msg(s.to_owned())
    }
}

impl From<String> for BitError {
    fn from(s: String) -> Self {
        Self::Msg(s)
    }
}
