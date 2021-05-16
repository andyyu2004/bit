use crate::obj::{BitId, Oid, PartialOid};
use owo_colors::OwoColorize;
use std::fmt::{self, Display, Formatter};

pub type BitResult<T> = Result<T, BitGenericError>;
pub type BitGenericError = anyhow::Error;

// usually we can just use anyhow for errors, but sometimes its nice to have a "rust" representation we can test or match against
#[derive(Debug)]
pub enum BitError {
    ObjectNotFound(BitId),
    /// object `{0}` not found in pack index but could be inserted at `{1}`
    ObjectNotFoundInPackIndex(Oid, u64),
    AmbiguousPrefix(PartialOid, Vec<Oid>),
}

pub trait BitErrorExt {
    fn into_obj_not_found_in_pack_index_err(self) -> BitResult<(Oid, u64)>;
}

impl BitErrorExt for BitGenericError {
    /// tries to convert generic error into specific error and just returns previous error on failure
    // this pattern feels pretty shit, not sure of a better way atm
    // usually don't have to catch errors that often so its not too bad (yet?)
    fn into_obj_not_found_in_pack_index_err(self) -> BitResult<(Oid, u64)> {
        match self.downcast::<BitError>() {
            Ok(BitError::ObjectNotFoundInPackIndex(oid, idx)) => Ok((oid, idx)),
            Ok(err) => Err(anyhow!(err)),
            Err(err) => Err(err),
        }
    }
}

pub trait BitResultExt {
    fn is_not_found_err(&self) -> bool;
    fn is_fatal(&self) -> bool;
}

macro_rules! error_ext_is_method {
    ($method:ident) => {
        fn $method(&self) -> bool {
            match self {
                Ok(..) => false,
                Err(err) => err.$method(),
            }
        }
    };
}

impl<T> BitResultExt for BitResult<T> {
    error_ext_is_method!(is_not_found_err);

    error_ext_is_method!(is_fatal);
}

impl BitResultExt for BitGenericError {
    fn is_not_found_err(&self) -> bool {
        match self.downcast_ref::<BitError>() {
            Some(err) => match err {
                BitError::ObjectNotFound(..) => true,
                BitError::ObjectNotFoundInPackIndex(..) => true,
                _ => false,
            },
            None => false,
        }
    }

    fn is_fatal(&self) -> bool {
        match self.downcast_ref::<BitError>() {
            Some(err) => match err {
                BitError::ObjectNotFound(..) => false,
                BitError::ObjectNotFoundInPackIndex(..) => false,
                _ => true,
            },
            None => true,
        }
    }
}

macro_rules! write_hint {
    ($f:expr, $($args:tt)*) => {{
        write!($f, "{}: ", "hint".yellow())?;
        writeln!($f, $($args)*)?
    }};
}

impl Display for BitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitError::ObjectNotFound(id) => write!(f, "bit object with hash `{}` not found", id),
            BitError::ObjectNotFoundInPackIndex(..) => unreachable!("not a user facing error"),
            BitError::AmbiguousPrefix(prefix, candidates) => {
                writeln!(f, "prefix oid `{}` is ambiguous", prefix)?;
                write_hint!(f, "the candidates are:");
                for candidate in candidates {
                    write_hint!(f, "  {}", candidate.yellow());
                }
                Ok(())
            }
        }
    }
}
