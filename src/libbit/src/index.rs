use crate::error::BitResult;
use std::io::prelude::*;

// refer, version: (), entryc: ()  version: (), entryc: () version: (), entryc: ()to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
struct BitIndex {
    header: BitIndexHeader,
}

#[derive(Debug, PartialEq)]
struct BitIndexHeader {
    signature: [u8; 4],
    version: u32,
    entryc: u32,
}

impl BitIndex {
    fn parse_header<R: BufRead>(r: &mut R) -> BitResult<BitIndexHeader> {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        let signature = buf;
        r.read_exact(&mut buf)?;
        let version = u32::from_be_bytes(buf);
        r.read_exact(&mut buf)?;
        let entryc = u32::from_be_bytes(buf);
        Ok(BitIndexHeader { signature, version, entryc })
    }

    fn deserialize_buffered<R: BufRead>(r: &mut R) -> BitIndex {
        let header = Self::parse_header(r)?;
        Self { header }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn read_index_header() -> BitResult<()> {
        let bytes = include_bytes!("../tests/files/index") as &[u8];
        let header = BitIndex::parse_header(&mut BufReader::new(bytes))?;
        assert_eq!(
            header,
            BitIndexHeader { signature: [b'D', b'I', b'R', b'C'], version: 2, entryc: 50 }
        );
        Ok(())
    }
}
