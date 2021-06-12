use thiserror::Error;

pub type LogicResult<T> = Result<T, LogicError>;

impl From<String> for LogicError {
    fn from(s: String) -> Self {
        LogicError(s)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Error)]
#[error("{0}")]
pub struct LogicError(String);
