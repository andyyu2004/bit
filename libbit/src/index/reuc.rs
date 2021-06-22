use crate::error::BitResult;
use crate::io::ReadExt;
use crate::serialize::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(test, derive(BitArbitrary))]
pub struct BitReuc {
    // TODO not implemented
    data: Vec<u8>,
}

impl Serialize for BitReuc {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writer.write_all(&self.data)?)
    }
}

impl Deserialize for BitReuc {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(Self { data: reader.read_to_vec()? })
    }
}
