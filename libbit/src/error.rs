use crate::obj::{BitId, Oid, PartialOid};
use crate::refs::SymbolicRef;
use crate::status::BitStatus;
use owo_colors::OwoColorize;
use std::fmt::{self, Display, Formatter};

pub type BitResult<T> = Result<T, BitGenericError>;
pub type BitGenericError = anyhow::Error;

// usually we can just use anyhow for errors, but sometimes its nice to have a "rust" representation we can test or match against
// consider not even using an enum and just have top level structs as this is resulting in extra unnecessary indirection
#[derive(Debug, PartialEq)]
pub enum BitError {
    ObjectNotFound(BitId),
    /// object `{0}` not found in pack index but could be inserted at `{1}`
    ObjectNotFoundInPackIndex(Oid, u64),
    AmbiguousPrefix(PartialOid, Vec<Oid>),
    NonExistentSymRef(SymbolicRef),
}

pub trait BitErrorExt {
    fn try_into_obj_not_found_in_pack_index_err(self) -> BitResult<(Oid, u64)>;
    fn try_into_obj_not_found_err(self) -> BitResult<BitId>;
    fn try_into_nonexistent_symref_err(self) -> BitResult<SymbolicRef>;
    fn try_into_bit_error(self) -> BitResult<BitError>;
    fn try_into_status_error(self) -> BitResult<BitStatus>;
}

impl BitErrorExt for BitGenericError {
    /// tries to convert generic error into specific error and just returns previous error on failure
    // this pattern feels pretty shit, not sure of a better way atm
    // usually don't have to catch errors that often so its not too bad (yet?)
    fn try_into_obj_not_found_in_pack_index_err(self) -> BitResult<(Oid, u64)> {
        match self.try_into_bit_error()? {
            BitError::ObjectNotFoundInPackIndex(oid, idx) => Ok((oid, idx)),
            err => Err(anyhow!(err)),
        }
    }

    fn try_into_nonexistent_symref_err(self) -> BitResult<SymbolicRef> {
        match self.try_into_bit_error()? {
            BitError::NonExistentSymRef(sym) => Ok(sym),
            err => Err(anyhow!(err)),
        }
    }

    fn try_into_bit_error(self) -> BitResult<BitError> {
        match self.downcast::<BitError>() {
            Ok(bit_error) => Ok(bit_error),
            Err(cast_failed_err) => Err(cast_failed_err),
        }
    }

    fn try_into_status_error(self) -> BitResult<BitStatus> {
        self.downcast()
    }

    fn try_into_obj_not_found_err(self) -> BitResult<BitId> {
        match self.try_into_bit_error()? {
            BitError::ObjectNotFound(id) => Ok(id),
            err => Err(anyhow!(err)),
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
            Some(err) => matches!(
                err,
                BitError::ObjectNotFound(..) | BitError::ObjectNotFoundInPackIndex(..)
            ),
            None => false,
        }
    }

    fn is_fatal(&self) -> bool {
        match self.downcast_ref::<BitError>() {
            Some(err) => !matches!(
                err,
                BitError::ObjectNotFound(..) | BitError::ObjectNotFoundInPackIndex(..)
            ),
            None => true,
        }
    }
}

macro_rules! write_hint {
    ($f:expr, $($args:tt)*) => {{
        write!($f, "{}: ", "hint".yellow())?;
        writeln!($f, $($args)*)
    }};
}

impl Display for BitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BitError::ObjectNotFound(id) => write!(f, "bit object with hash `{}` not found", id),
            BitError::ObjectNotFoundInPackIndex(..) => unreachable!("not a user facing error"),
            BitError::AmbiguousPrefix(prefix, candidates) => {
                writeln!(f, "prefix oid `{}` is ambiguous", prefix)?;
                write_hint!(f, "the candidates are:")?;
                for candidate in candidates {
                    write_hint!(f, "  {}", candidate.yellow())?;
                }
                Ok(())
            }
            BitError::NonExistentSymRef(sym) =>
                writeln!(f, "failed to resolve reference `{}`", sym),
        }
    }
}
