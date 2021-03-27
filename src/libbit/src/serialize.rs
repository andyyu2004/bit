use crate::error::BitResult;
use std::io::Write;

pub trait Serialize {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()>;
}
