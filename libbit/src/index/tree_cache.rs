use crate::error::BitResult;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::serialize::Deserialize;
use std::io::BufRead;

#[derive(Debug)]
pub struct BitTreeCache {
    path: BitPath,
    children: Vec<BitTreeCache>,
    // -1 means invalid
    entry_count: isize,
    oid: Oid,
}

impl Deserialize for BitTreeCache {
    fn deserialize(reader: &mut impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}
