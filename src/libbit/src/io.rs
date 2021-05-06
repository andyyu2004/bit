use crate::error::BitResult;
use crate::hash::{BitHash, SHA1Hash};
use crate::serialize::Deserialize;
use crate::time::Timespec;
use sha1::{digest::Output, Digest};
use std::io::{self, prelude::*};
use std::mem::MaybeUninit;

// all big-endian
pub(crate) trait ReadExt: Read {
    fn read_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn read_timespec(&mut self) -> io::Result<Timespec> {
        let sec = self.read_u32()?;
        let nano = self.read_u32()?;
        Ok(Timespec::new(sec, nano))
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    fn read_bit_hash(&mut self) -> io::Result<BitHash> {
        let mut buf = [0u8; 20];
        self.read_exact(&mut buf)?;
        Ok(BitHash::new(buf))
    }

    fn read_to_vec(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = vec![];
        self.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl<R: Read + ?Sized> ReadExt for R {
}

impl Deserialize for u64 {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_u64()?)
    }
}

impl Deserialize for u32 {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_u32()?)
    }
}

impl Deserialize for BitHash {
    fn deserialize(reader: &mut dyn BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_bit_hash()?)
    }
}

pub trait BufReadExt: BufRead + Sized {
    fn read_array<T: Deserialize, const N: usize>(&mut self) -> BitResult<[T; N]> {
        // SAFETY? not sure
        // let mut xs: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut xs: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };
        for i in 0..N {
            xs[i] = MaybeUninit::new(T::deserialize(self)?);
        }
        // shouldn't be necessary to forget since everything is MaybeUninit in xs?
        // std::mem::forget();
        // can't transmute do to const generics atm even though
        // https://github.com/rust-lang/rust/issues/61956
        Ok(unsafe { std::mem::transmute_copy(&xs) })
    }

    fn is_at_eof(&mut self) -> io::Result<bool> {
        Ok(self.fill_buf()?.is_empty())
    }

    fn read_type<T: Deserialize>(&mut self) -> BitResult<T> {
        T::deserialize(self)
    }

    fn read_vec<T: Deserialize>(&mut self, n: usize) -> BitResult<Vec<T>> {
        (0..n).map(|_| T::deserialize(self)).collect::<Result<_, _>>()
    }
}

impl<R: BufRead> BufReadExt for R {
}

pub trait WriteExt: Write {
    fn write_u16(&mut self, u: u16) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_u32(&mut self, u: u32) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_timespec(&mut self, t: Timespec) -> io::Result<()> {
        self.write_u32(t.sec)?;
        self.write_u32(t.nano)
    }

    fn write_u64(&mut self, u: u64) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_bit_hash(&mut self, hash: &BitHash) -> io::Result<()> {
        self.write_all(hash.as_bytes())
    }
}

impl<W: Write + ?Sized> WriteExt for W {
}

pub(crate) struct HashReader<'a, D> {
    reader: &'a mut dyn BufRead,
    hasher: D,
}

impl<'a, D: Digest> BufRead for HashReader<'a, D> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt)
    }
}

impl<'a, D: Digest> Read for HashReader<'a, D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

impl<'a, D: Digest> HashReader<'a, D> {
    pub fn finalize_hash(&mut self) -> Output<D> {
        self.hasher.finalize_reset()
    }

    pub fn new(reader: &'a mut dyn BufRead) -> Self {
        Self { reader, hasher: D::new() }
    }
}

impl<'a> HashReader<'a, sha1::Sha1> {
    pub fn new_sha1(reader: &'a mut dyn BufRead) -> Self {
        Self::new(reader)
    }

    pub fn finalize_sha1_hash(&mut self) -> SHA1Hash {
        SHA1Hash::from(self.hasher.finalize_reset())
    }
}

/// hashes all the bytes written into the writer
pub(crate) struct HashWriter<'a, D> {
    writer: &'a mut dyn Write,
    hasher: D,
}

impl<'a, D: Digest> Write for HashWriter<'a, D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<'a, D: Digest> HashWriter<'a, D> {
    pub fn finalize_hash(&mut self) -> Output<D> {
        self.hasher.finalize_reset()
    }

    pub fn new(writer: &'a mut dyn Write) -> Self {
        Self { writer, hasher: D::new() }
    }
}

impl<'a> HashWriter<'a, sha1::Sha1> {
    pub fn new_sha1(writer: &'a mut dyn Write) -> Self {
        Self::new(writer)
    }

    pub fn finalize_sha1_hash(&mut self) -> SHA1Hash {
        SHA1Hash::from(self.hasher.finalize_reset())
    }
}

#[cfg(test)]
mod tests;
