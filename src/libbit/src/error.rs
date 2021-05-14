use crate::obj::{BitId, Oid};
use std::fmt::{self, Display, Formatter};

pub type BitResult<T> = Result<T, BitGenericError>;
pub type BitGenericError = anyhow::Error;

#[derive(Debug)]
pub enum BitError {
    ObjectNotFound(BitId),
    /// object `{0}` not found in pack index but could be inserted at `{1}`
    ObjectNotFoundInPackIndex(Oid, usize),
}

pub trait BitErrorExt {
    fn is_not_found_err(&self) -> bool;
    fn is_fatal(&self) -> bool;
}

macro_rules! error_ext_method {
    ($method:ident) => {
        fn $method(&self) -> bool {
            match self {
                Ok(..) => false,
                Err(err) => err.$method(),
            }
        }
    };
}

impl<T> BitErrorExt for BitResult<T> {
    error_ext_method!(is_not_found_err);

    error_ext_method!(is_fatal);
}

impl BitErrorExt for BitGenericError {
    fn is_not_found_err(&self) -> bool {
        match self.downcast_ref::<BitError>() {
            Some(err) => match err {
                BitError::ObjectNotFound(..) => true,
                BitError::ObjectNotFoundInPackIndex(..) => true,
            },
            None => false,
        }
    }

    fn is_fatal(&self) -> bool {
        match self.downcast_ref::<BitError>() {
            Some(err) => match err {
                BitError::ObjectNotFound(..) => false,
                BitError::ObjectNotFoundInPackIndex(..) => false,
            },
            None => true,
        }
    }
}

impl Display for BitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitError::ObjectNotFound(id) => write!(f, "bit object with hash `{}` not found", id),
            BitError::ObjectNotFoundInPackIndex(..) => unreachable!("not a user facing error"),
        }
    }
}

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
