use crate::hash::BitHash;
use sha1::{digest::Output, Digest};
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

/// hashes all the bytes written into the writer
pub(crate) struct HashWriter<'a, D, W> {
    writer: &'a mut W,
    hasher: D,
}

impl<'a, D: Digest, W: Write> Write for HashWriter<'a, D, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl<'a, D: Digest, W: Write> HashWriter<'a, D, W> {
    pub fn finalize_hash(&mut self) -> Output<D> {
        self.hasher.finalize_reset()
    }

    pub fn new(writer: &'a mut W) -> Self {
        Self { writer, hasher: D::new() }
    }
}

impl<'a, W: Write> HashWriter<'a, sha1::Sha1, W> {
    pub fn new_sha1(writer: &'a mut W) -> Self {
        Self::new(writer)
    }
}
