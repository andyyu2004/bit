use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::{BitObj, BitObjType};
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

// using a string to represent this for now as its a bit confusing
// 100644 normal (roughly)
// 100755 executable (roughly)
// 40000 means directory?
#[derive(Debug, PartialEq, Clone)]
pub struct FileMode(String);

impl Display for FileMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            for entry in &self.entries {
                write!(f, "{:#}", entry)?;
            }
        } else {
            todo!()
        }
        Ok(())
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{} {}\0{}", self.mode, self.path.display(), unsafe {
                // SAFETY we're just printing this out and not using it anywhere
                std::str::from_utf8_unchecked(self.hash.as_ref())
            })
        } else {
            todo!()
        }
    }
}

#[derive(PartialEq, Debug, Default, Clone)]
pub struct Tree {
    // maybe could be a map of hash to tree entry?
    entries: Vec<TreeEntry>,
}

impl BitObj for Tree {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }

    fn deserialize<R: Read>(r: R) -> BitResult<Self> {
        let mut r = BufReader::new(r);
        let mut tree = Self::default();

        // slightly weird way of checking if the reader is at EOF
        while r.fill_buf()? != &[] {
            tree.entries.push(TreeEntry::parse(&mut r)?);
        }
        Ok(tree)
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Tree
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct TreeEntry {
    mode: FileMode,
    path: PathBuf,
    hash: BitHash,
}

impl TreeEntry {
    pub fn parse<R: BufRead>(r: &mut R) -> BitResult<Self> {
        let mut buf = vec![];
        let i = r.read_until(0x20, &mut buf)?;
        let mode = FileMode(std::str::from_utf8(&buf[..i - 1]).unwrap().to_owned());

        let j = r.read_until(0x00, &mut buf)?;
        // fairly disgusting way of deserializing a path..
        let path = PathBuf::from(OsString::from(std::str::from_utf8(&buf[i..i + j - 1]).unwrap()));

        let mut hash_bytes = [0; 20];
        r.read_exact(&mut hash_bytes)?;
        let hash = BitHash::new(hash_bytes);
        // assert_eq!(r.read_until(0x00, &mut buf)?, 1);
        Ok(Self { mode, path, hash })
    }

    pub fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        // TODO will mode always be exactly 6 bytes or is it an upper bound?
        write!(writer, "{}", self.mode)?;
        writer.write_all(b" ")?;
        write!(writer, "{}", self.path.display())?;
        writer.write_all(b"\0")?;
        writer.write_all(self.hash.as_ref())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    impl Arbitrary for FileMode {
        fn arbitrary(g: &mut Gen) -> Self {
            Self("10644".to_owned())
        }
    }

    impl Arbitrary for TreeEntry {
        fn arbitrary(g: &mut Gen) -> Self {
            Self {
                path: PathBuf::from(generate_sane_string(1..300)),
                mode: Arbitrary::arbitrary(g),
                hash: Arbitrary::arbitrary(g),
            }
        }
    }

    impl Arbitrary for Tree {
        fn arbitrary(g: &mut Gen) -> Self {
            Self { entries: Arbitrary::arbitrary(g) }
        }
    }

    #[quickcheck]
    fn serialize_then_parse_tree(tree: Tree) -> BitResult<()> {
        let mut bytes = vec![];
        tree.serialize(&mut bytes)?;
        let parsed = Tree::deserialize(bytes.as_slice())?;
        assert_eq!(tree, parsed);
        Ok(())
    }

    #[test]
    fn parse_then_serialize_tree() -> BitResult<()> {
        let bytes = include_bytes!("../../tests/files/testtree.tree") as &[u8];
        let tree = Tree::deserialize(bytes)?;
        let mut serialized = vec![];
        tree.serialize(&mut serialized)?;
        assert_eq!(bytes, serialized);
        Ok(())
    }
}
