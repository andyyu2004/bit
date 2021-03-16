use super::FileMode;
use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::{BitObj, BitObjType};
use crate::path::BitPath;
use crate::serialize::Serialize;
use crate::tls;
use crate::util;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            for entry in &self.entries {
                write!(f, "{:#}", entry)?;
            }
        } else {
            for entry in &self.entries {
                writeln!(f, "{}", entry)?;
            }
        }
        Ok(())
    }
}

impl Display for TreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{} {}\0{}", self.mode, self.path, unsafe {
                // SAFETY we're just printing this out and not using it anywhere
                std::str::from_utf8_unchecked(self.hash.as_ref())
            })
        } else {
            let ty = tls::REPO.with(|repo| repo.read_obj_type_from_hash(&self.hash)).unwrap();
            write!(f, "{} {} {}\t{}", self.mode, ty, self.hash, self.path)
        }
    }
}

#[derive(PartialEq, Debug, Default, Clone)]
pub struct Tree {
    pub entries: BTreeSet<TreeEntry>,
}

impl Serialize for Tree {
    fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }
}

impl BitObj for Tree {
    fn deserialize_buffered<R: BufRead>(r: &mut R) -> BitResult<Self> {
        let mut tree = Self::default();

        #[cfg(debug_assertions)]
        let mut v = vec![];

        // slightly weird way of checking if the reader is at EOF
        while r.fill_buf()? != &[] {
            let entry = TreeEntry::parse(r)?;
            #[cfg(debug_assertions)]
            v.push(entry.clone());
            tree.entries.insert(entry);
        }

        // these debug assertions are checking that the btreeset ordering
        // is consistent with the order of the tree entries on disk
        #[cfg(debug_assertions)]
        assert_eq!(tree.entries.iter().cloned().collect::<Vec<_>>(), v);
        Ok(tree)
    }

    fn obj_ty(&self) -> BitObjType {
        BitObjType::Tree
    }
}

#[derive(PartialEq, Debug, Clone, Eq)]
pub struct TreeEntry {
    pub mode: FileMode,
    pub path: BitPath,
    pub hash: BitHash,
}

impl PartialOrd for TreeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TreeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path.cmp(&other.path).then_with(|| {
            // this slightly odd code is emulating what is done in libgit2 (sort of)
            let c1 = if self.mode.is_dir() { '/' } else { '\0' };
            let c2 = if other.mode.is_dir() { '/' } else { '\0' };
            c1.cmp(&c2)
        })
    }
}

impl TreeEntry {
    pub fn parse<R: BufRead>(r: &mut R) -> BitResult<Self> {
        let mut buf = vec![];
        let i = r.read_until(0x20, &mut buf)?;
        let mode =
            FileMode(u32::from_str_radix(std::str::from_utf8(&buf[..i - 1]).unwrap(), 8).unwrap());

        let j = r.read_until(0x00, &mut buf)?;
        // fairly disgusting way of deserializing a path..
        let path = util::path_from_bytes(&buf[i..i + j - 1]);

        let mut hash_bytes = [0; 20];
        r.read_exact(&mut hash_bytes)?;
        let hash = BitHash::new(hash_bytes);
        // assert_eq!(r.read_until(0x00, &mut buf)?, 1);
        Ok(Self { mode, path, hash })
    }

    pub fn serialize<W: Write>(&self, writer: &mut W) -> BitResult<()> {
        // use alternate display impl to not pad an extra 0
        write!(writer, "{:#}", self.mode)?;
        writer.write_all(b" ")?;
        write!(writer, "{}", self.path)?;
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
        fn arbitrary(_g: &mut Gen) -> Self {
            Self(0100644)
        }
    }

    impl Arbitrary for TreeEntry {
        fn arbitrary(g: &mut Gen) -> Self {
            Self {
                path: BitPath::intern(&generate_sane_string(1..300)),
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
        // this tree was generated by git
        let bytes = include_bytes!("../../tests/files/testtree.tree") as &[u8];
        let tree = Tree::deserialize(bytes)?;
        let mut serialized = vec![];
        tree.serialize(&mut serialized)?;
        assert_eq!(bytes, serialized);
        Ok(())
    }
}
