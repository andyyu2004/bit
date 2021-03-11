use crate::hash::BitHash;
use std::io::prelude::*;

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

pub(crate) trait WriteExt: Write {
    fn write_u16(&mut self, u: u16) -> std::io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_u32(&mut self, u: u32) -> std::io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_u64(&mut self, u: u64) -> std::io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_bit_hash(&mut self, hash: &BitHash) -> std::io::Result<()> {
        self.write_all(hash.as_bytes())
    }
}

impl<T: Write> WriteExt for T {
}
