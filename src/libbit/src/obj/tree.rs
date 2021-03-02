use crate::error::BitResult;
use crate::hash::BitHash;
use std::convert::TryInto;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct FileMode([u8; 6]);

impl Display for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for &byte in self.as_ref() {
            write!(f, "{}", byte.to_ascii_lowercase())?;
        }
        Ok(())
    }
}

impl AsRef<[u8]> for FileMode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for entry in &self.entries {
            write!(f, "{}", entry)?;
        }
        Ok(())
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} <tree-entry>", self.mode)
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct Tree {
    // maybe could be a map of hash to tree entry?
    entries: Vec<TreeEntry>,
}

impl Tree {
    pub fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }

    pub fn parse<R: Read>(r: R) -> BitResult<Self> {
        let mut r = BufReader::new(r);
        let mut tree = Self::default();

        // slightly weird way of checking if the reader is at EOF
        while r.fill_buf()? != &[] {
            tree.entries.push(TreeEntry::parse(&mut r)?)
        }
        Ok(tree)
    }
}

#[derive(PartialEq, Debug)]
pub struct TreeEntry {
    mode: FileMode,
    path: PathBuf,
    hash: BitHash,
}

impl TreeEntry {
    pub fn parse<R: BufRead>(r: &mut R) -> BitResult<Self> {
        let mut buf = vec![];
        let i = r.read_until(0x20, &mut buf)?;
        assert_eq!(i, 6, "filemode was not 6 bytes long");
        let mode = FileMode(buf[0..i].try_into().unwrap());

        let j = r.read_until(0x00, &mut buf)?;
        // fairly disgusting way of deserializing a path..
        let path = PathBuf::from(OsString::from(std::str::from_utf8(&buf[i + 1..j]).unwrap()));

        let mut hash_bytes = [0; 20];
        r.read_exact(&mut hash_bytes)?;
        let hash = BitHash::new(hash_bytes);
        Ok(Self { mode, path, hash })
    }

    pub fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        // TODO will mode always be exactly 6 bytes or is it an upper bound?
        write!(writer, "{}", self.mode)?;
        writer.write_all(b" ")?;
        write!(writer, "{}", self.path.display())?;
        writer.write_all(self.hash.as_ref())?;
        Ok(())
    }
}
