use std::io::Read;

use crate::hash::BitHash;

// all big-endian
pub(crate) trait ReadExt: Read {
    fn read_u16(&mut self) -> std::io::Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn read_u32(&mut self) -> std::io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn read_u64(&mut self) -> std::io::Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    fn read_bit_hash(&mut self) -> std::io::Result<BitHash> {
        let mut buf = [0u8; 20];
        self.read_exact(&mut buf)?;
        Ok(BitHash::new(buf))
    }
}

impl<T: Read> ReadExt for T {
}
