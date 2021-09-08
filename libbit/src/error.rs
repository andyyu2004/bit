use crate::checkout::CheckoutConflicts;
use crate::merge::MergeConflict;
use crate::obj::{BitId, BitObjType, Oid, PartialOid};
use crate::refs::SymbolicRef;
use crate::status::BitStatus;
use owo_colors::OwoColorize;
use std::fmt::{self, Display, Formatter};

pub type BitResult<T> = Result<T, BitGenericError>;
pub type BitGenericError = anyhow::Error;

// usually we can just use anyhow for errors, but sometimes its nice to have a "rust" representation we can test or match against
// consider not even using an enum and just have top level structs as this is resulting in extra unnecessary indirection
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum BitError {
    ObjectNotFound(BitId),
    /// object `{0}` not found in pack index but could be inserted at `{1}`
    ObjectNotFoundInPackIndex(Oid, u64),
    AmbiguousPrefix(PartialOid, Vec<Oid>),
    NonExistentSymRef(SymbolicRef),
    MergeConflict(MergeConflict),
    CheckoutConflict(CheckoutConflicts),
    ExpectedCommit(Oid, BitObjType),
    PackBackendWrite,
}

pub trait BitErrorExt {
    fn try_into_obj_not_found_in_pack_index_err(self) -> BitResult<(Oid, u64)>;
    fn try_into_obj_not_found_err(self) -> BitResult<BitId>;
    fn try_into_nonexistent_symref_err(self) -> BitResult<SymbolicRef>;
    fn try_into_bit_error(self) -> BitResult<BitError>;
    fn try_into_status_error(self) -> BitResult<BitStatus>;
    fn try_into_expected_commit_error(self) -> BitResult<(Oid, BitObjType)>;
    fn try_into_merge_conflict(self) -> BitResult<MergeConflict>;
    fn try_into_checkout_conflict(self) -> BitResult<CheckoutConflicts>;
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

    fn try_into_checkout_conflict(self) -> BitResult<CheckoutConflicts> {
        match self.try_into_bit_error()? {
            BitError::CheckoutConflict(checkout_conflict) => Ok(checkout_conflict),
            err => Err(anyhow!(err)),
        }
    }

    fn try_into_merge_conflict(self) -> BitResult<MergeConflict> {
        match self.try_into_bit_error()? {
            BitError::MergeConflict(merge_conflict) => Ok(merge_conflict),
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

    fn try_into_expected_commit_error(self) -> BitResult<(Oid, BitObjType)> {
        match self.try_into_bit_error()? {
            BitError::ExpectedCommit(oid, obj_type) => Ok((oid, obj_type)),
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
                BitError::ObjectNotFound(..)
                    | BitError::ObjectNotFoundInPackIndex(..)
                    | BitError::MergeConflict(..)
                    | BitError::CheckoutConflict(..)
                    | BitError::PackBackendWrite
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
            BitError::AmbiguousPrefix(prefix, candidates) => {
                writeln!(f, "prefix oid `{}` is ambiguous", prefix)?;
                write_hint!(f, "the candidates are:")?;
                for candidate in candidates {
                    write_hint!(f, "  {}", candidate.yellow())?;
                }
                Ok(())
            }
            BitError::NonExistentSymRef(sym) =>
                write!(f, "failed to resolve symbolic reference `{}`", sym),
            BitError::MergeConflict(merge_conflict) => write!(f, "{}", merge_conflict),
            BitError::PackBackendWrite | BitError::ObjectNotFoundInPackIndex(..) =>
                bug!("not a user facing error"),
            BitError::CheckoutConflict(conflicts) => {
                // TODO
                writeln!(f, "some checkout conflicts: {:?}", conflicts)
            }
            BitError::ExpectedCommit(oid, obj_type) =>
                writeln!(f, "`{}` is a {}, expected commit", oid, obj_type),
        }
    }
}
