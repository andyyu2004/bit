use crate::error::BitGenericError;
use crate::hash::SHA1Hash;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::serialize::Deserialize;
use crate::time::Timespec;
use crate::{error::BitResult, serialize::Serialize};
use sha1::Digest;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use std::mem::MaybeUninit;
use std::os::unix::prelude::OsStrExt;
use std::str::FromStr;

pub type BufferedFileStream = std::io::BufReader<File>;

// all big-endian
pub(crate) trait ReadExt: Read {
    #[inline]
    fn read_u8(&mut self) -> io::Result<u8> {
        let mut i = 0u8;
        self.read_exact(&mut std::slice::from_mut(&mut i))?;
        Ok(i)
    }

    /// read offset encoding used for [crate::obj::BitObjKind::OfsDelta]
    // pretty weird encoding
    // https://medium.com/@concertdaw/sneaky-git-number-encoding-ddcc5db5329f
    // https://github.com/git/git/blob/26e47e261e969491ad4e3b6c298450c061749c9e/builtin/pack-objects.c#L1443-L1473
    fn read_offset(&mut self) -> io::Result<u64> {
        let mut byte = self.read_u8()? as u64;
        let mut offset = byte & 0x7f;
        while byte & 0x80 != 0 {
            offset += 1;
            byte = self.read_u8()? as u64;
            offset = (offset << 7) | (byte & 0x7f);
        }
        Ok(offset)
    }

    #[inline]
    /// alias for `read_le_varint` with a more intuitive name
    fn read_size(&mut self) -> io::Result<u64> {
        self.read_le_varint()
    }

    #[inline]
    // variable length little-endian integer encoding
    // read next byte if MSB is 1
    // referred to as "size encoding" in git docs
    fn read_le_varint(&mut self) -> io::Result<u64> {
        self.read_le_varint_with_shift(0).map(|x| x.1)
    }

    // shift is useful for if there is another number encoded in the first few bits
    fn read_le_varint_with_shift(&mut self, init_shift: u64) -> io::Result<(u8, u64)> {
        // cannot shift more than 7 as the MSB is reserved
        assert!(init_shift < 8);
        // example with shift = 3
        // 0x11010010
        //    ^^^  these are the leading bits we want to extract separately
        // we use `k_mask` below to do this
        // the first time in the loop we need to mask out the remaining bits
        // in the remaining loops we reset the mask to 0x7f which is everyting except MSB

        let mut n = 0;
        let byte = self.read_u8()?;
        let anti_shift = 7 - init_shift;
        let k_mask = ((1 << init_shift) - 1) << anti_shift;
        let k = (byte & k_mask as u8) >> anti_shift;

        // process the remaining few bits of the first byte
        let mask = (1 << anti_shift) - 1;
        n |= (byte & mask) as u64;

        // only continue if the first bits MSB is 1
        if byte & 0x80 != 0 {
            let mut shift = 7 - init_shift;
            loop {
                let byte = self.read_u8()? as u64;
                n |= (byte & 0x7f) << shift;
                shift += 7;
                if byte & 0x80 == 0 {
                    break;
                }
            }
        }

        Ok((k, n))
    }

    /// format used for encoding delta copy operaion
    /// header must have the MSB set (otherwise we shouldn't be reading this format)
    /// format on disk (in `self`) is as follows
    /// +----------+---------+---------+---------+---------+-------+-------+-------+
    /// | 1xxxxxxx | offset1 | offset2 | offset3 | offset4 | size1 | size2 | size3 |
    /// +----------+---------+---------+---------+---------+-------+-------+-------+
    /// if bit zero(lsb) is set, then offset1 is present etc..
    // we choose to read all 7 bits in little endian so be wary when extracting
    // size and offset!
    fn read_le_packed(&mut self, header: u8) -> io::Result<u64> {
        debug_assert!(header & 1 << 7 != 0);
        let mut value = 0;
        for i in 0..7 {
            if header & 1 << i == 0 {
                continue;
            }

            let byte = self.read_u8()? as u64;
            value |= byte << (i * 8)
        }
        Ok(value)
    }

    #[inline]
    fn read_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    #[inline]
    fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    #[inline]
    fn read_timespec(&mut self) -> io::Result<Timespec> {
        let sec = self.read_u32()?;
        let nano = self.read_u32()?;
        Ok(Timespec::new(sec, nano))
    }

    #[inline]
    fn read_u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    #[inline]
    fn read_oid(&mut self) -> io::Result<Oid> {
        let mut buf = [0u8; 20];
        self.read_exact(&mut buf)?;
        Ok(Oid::new(buf))
    }

    #[inline]
    // named str to not clash with the existing method
    fn read_to_str(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.read_to_string(&mut buf)?;
        Ok(buf)
    }

    #[inline]
    fn read_to_vec(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = vec![];
        self.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

impl<R: Read + ?Sized> ReadExt for R {
}

impl Deserialize for u64 {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_u64()?)
    }
}

impl Deserialize for u8 {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_u8()?)
    }
}

impl Deserialize for u32 {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_u32()?)
    }
}

impl Deserialize for Oid {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_oid()?)
    }
}

impl Deserialize for Vec<u8> {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        Ok(reader.read_to_vec()?)
    }
}

#[cfg(test)]
impl Serialize for Vec<u8> {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writer.write_all(self)?)
    }
}

impl Serialize for [u8] {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writer.write_all(self)?)
    }
}

// this trait exists as we passing `self` to `T::deserialize` which takes a `dyn mut BufRead`
// requires `Self: Sized`. Not entirely sure why atm.
pub trait BufReadExtSized: BufRead + Sized {
    fn read_array<T: Deserialize, const N: usize>(&mut self) -> BitResult<[T; N]> {
        // SAFETY? not sure
        let mut xs: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };
        for i in 0..N {
            xs[i] = MaybeUninit::new(T::deserialize(&mut *self)?);
        }
        // shouldn't be necessary to forget since everything is MaybeUninit in xs?
        // std::mem::forget();
        // can't transmute do to const generics atm even though
        // https://github.com/rust-lang/rust/issues/61956
        Ok(unsafe { std::mem::transmute_copy(&xs) })
    }

    fn read_type<T: Deserialize>(&mut self) -> BitResult<T> {
        T::deserialize(self)
    }

    fn read_vec<T: Deserialize>(&mut self, n: usize) -> BitResult<Vec<T>> {
        let mut vec = Vec::with_capacity(n);
        for _ in 0..n {
            vec.push(T::deserialize(&mut *self)?);
        }
        Ok(vec)
    }
}

impl<R: BufRead> BufReadExtSized for R {
}

pub trait BufReadExt: BufRead {
    fn as_zlib_decode_stream(&mut self) -> BufReader<flate2::bufread::ZlibDecoder<&mut Self>> {
        BufReader::new(flate2::bufread::ZlibDecoder::new(self))
    }

    /// read the bytes upto `sep` parsing as a base10 ascii numberj
    fn read_ascii_num(&mut self, sep: u8) -> BitResult<i64> {
        let mut buf = vec![];
        let i = self.read_until(sep, &mut buf)?;
        Ok(std::str::from_utf8(&buf[..i - 1]).unwrap().parse().unwrap())
    }

    /// read the bytes upto `sep` parsing as an ascii str
    fn read_ascii_str<T: FromStr<Err = BitGenericError>>(&mut self, sep: u8) -> BitResult<T> {
        let mut buf = vec![];
        let i = self.read_until(sep, &mut buf)?;
        std::str::from_utf8(&buf[..i - 1]).unwrap().parse()
    }

    fn read_null_terminated_path(&mut self) -> BitResult<BitPath> {
        self.read_null_terminated()
    }

    // `n` should be at most the length of the path to read excluding the null byte
    fn read_null_terminated_path_skip_n(&mut self, n: usize) -> BitResult<BitPath> {
        let mut buf = vec![0; n];
        // optimization when we know how many bytes we can read
        self.read_exact(&mut buf)?;
        self.read_until(0, &mut buf)?;
        // ignore the null character
        Ok(BitPath::intern(OsStr::from_bytes(&buf[..buf.len() - 1])))
    }

    fn read_null_terminated<T: Deserialize>(&mut self) -> BitResult<T> {
        let mut buf = vec![];
        self.read_until(0, &mut buf)?;
        // ignore the null character
        T::deserialize(BufReader::new(&buf[..buf.len() - 1]))
    }

    fn is_at_eof(&mut self) -> io::Result<bool> {
        Ok(self.fill_buf()?.is_empty())
    }
}

impl<R: BufRead + ?Sized> BufReadExt for R {
}

pub trait WriteExt: Write {
    fn write_u8(&mut self, u: u8) -> io::Result<()> {
        self.write_all(std::slice::from_ref(&u))
    }

    fn write_u16(&mut self, u: u16) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_u32(&mut self, u: u32) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_ascii_num(&mut self, i: impl Display, sep: u8) -> io::Result<()> {
        self.write_all(i.to_string().as_bytes())?;
        self.write_u8(sep)
    }

    fn write_timespec(&mut self, t: Timespec) -> io::Result<()> {
        self.write_u32(t.sec)?;
        self.write_u32(t.nano)
    }

    fn write_u64(&mut self, u: u64) -> io::Result<()> {
        self.write_all(&u.to_be_bytes())
    }

    fn write_null_terminated_path(&mut self, path: BitPath) -> io::Result<()> {
        self.write_all(path.as_bytes())?;
        self.write_u8(0)?;
        Ok(())
    }

    fn write_oid(&mut self, oid: Oid) -> io::Result<()> {
        self.write_all(oid.as_bytes())
    }

    /// write `data` prefixed by its serialized size in bytes as a u32
    fn write_with_size(&mut self, data: impl Serialize) -> BitResult<()> {
        let mut buf = vec![];
        data.serialize(&mut buf)?;

        self.write_u32(buf.len() as u32)?;
        self.write_all(&buf)?;
        Ok(())
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

/// hashes all the bytes written into the writer using `D`
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
    pub fn new(writer: &'a mut dyn Write) -> Self {
        Self { writer, hasher: D::new() }
    }
}

impl<'a> HashWriter<'a, sha1::Sha1> {
    pub fn new_sha1(writer: &'a mut dyn Write) -> Self {
        Self::new(writer)
    }

    pub fn write_hash(self) -> io::Result<()> {
        let hash = SHA1Hash::from(self.hasher.finalize());
        self.writer.write_oid(hash)
    }
}

#[cfg(test)]
mod tests;
