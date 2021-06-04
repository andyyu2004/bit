use super::FileMode;
use crate::error::BitResult;
use crate::io::BufReadExt;
use crate::obj::{BitObj, BitObjType, Oid};
use crate::path::BitPath;
use crate::serialize::{Deserialize, DeserializeSized, Serialize};
use crate::tls;
use crate::util;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;

pub trait Treeish {
    fn into_tree(self) -> BitResult<Tree>;
}

impl Treeish for Tree {
    fn into_tree(self) -> BitResult<Self> {
        Ok(self)
    }
}

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
            let obj_type = self.mode.infer_obj_type();
            debug_assert_eq!(
                obj_type,
                tls::with_repo(|repo| repo.read_obj_header(self.hash).unwrap().obj_type)
            );
            write!(f, "{} {} {}\t{}", self.mode, obj_type, self.hash, self.path)
        }
    }
}

#[derive(PartialEq, Debug, Default, Clone)]
pub struct Tree {
    pub entries: BTreeSet<TreeEntry>,
}

// impl IntoIterator for Tree {
//     type IntoIter = ();
//     type Item;
//     fn into_iter(self) -> Self::IntoIter {
//         todo!()
//     }
// }
// impl Tree {
//     pub fn iter(&self) -> impl BitIterator {
//         todo!()
//     }
// }

impl Serialize for Tree {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }
}

impl DeserializeSized for Tree {
    fn deserialize_sized(r: &mut impl BufRead, size: u64) -> BitResult<Self>
    where
        Self: Sized,
    {
        let r = &mut r.take(size);

        let mut tree = Self::default();
        #[cfg(debug_assertions)]
        let mut v = vec![];

        while !r.is_at_eof()? {
            let entry = TreeEntry::deserialize(r)?;
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
}

impl BitObj for Tree {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Tree
    }
}

#[derive(PartialEq, Debug, Clone, Eq, Copy)]
pub struct TreeEntry {
    pub mode: FileMode,
    pub path: BitPath,
    pub hash: Oid,
}

impl PartialOrd for TreeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TreeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_path().cmp(&other.sort_path())
    }
}

impl TreeEntry {
    // we must have files sorted before directories
    // i.e. index.rs < index/
    // however, the trailing slash is not actually stored in the tree entry path (TODO confirm against git)
    // we fix this by appending appending a slash

    fn sort_path(&self) -> BitPath {
        if self.mode == FileMode::DIR { self.path.join_trailing_slash() } else { self.path }
    }
}

impl Deserialize for TreeEntry {
    fn deserialize(r: &mut impl BufRead) -> BitResult<Self> {
        let mut buf = vec![];
        let i = r.read_until(0x20, &mut buf)?;
        let mode =
            FileMode(u32::from_str_radix(std::str::from_utf8(&buf[..i - 1]).unwrap(), 8).unwrap());

        let j = r.read_until(0x00, &mut buf)?;
        // fairly disgusting way of deserializing a path..
        let path = util::path_from_bytes(&buf[i..i + j - 1]);

        let mut hash_bytes = [0; 20];
        r.read_exact(&mut hash_bytes)?;
        let hash = Oid::new(hash_bytes);
        Ok(Self { mode, path, hash })
    }
}

impl Serialize for TreeEntry {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
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
                path: BitPath::intern(&generate_sane_string_with_newlines(1..300)),
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
        let parsed = Tree::deserialize_sized_unbuffered(bytes.as_slice(), bytes.len() as u64)?;
        assert_eq!(tree, parsed);
        Ok(())
    }

    #[test]
    fn parse_then_serialize_tree() -> BitResult<()> {
        // this tree was generated by git
        let bytes = include_bytes!("../../tests/files/testtree.tree") as &[u8];
        let tree = Tree::deserialize_sized_unbuffered(bytes, bytes.len() as u64)?;
        let mut serialized = vec![];
        tree.serialize(&mut serialized)?;
        assert_eq!(bytes, serialized);
        Ok(())
    }
}
