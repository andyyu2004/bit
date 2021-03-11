use crate::error::BitResult;
use std::io::Write;

pub trait Serialize {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()>;
}
