use crate::{BitRepo, BitResult};
use flate2::read::ZlibDecoder;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::str::FromStr;

enum ObjType {
    Commit,
    Tree,
    Tag,
    Blob,
}

impl FromStr for ObjType {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "commit" => ObjType::Commit,
            "tree" => ObjType::Tree,
            "tag" => ObjType::Tag,
            "blob" => ObjType::Blob,
            _ => panic!("unknown object type `{}`", s),
        })
    }
}

pub fn read_obj<R: Read>(read: R) -> BitResult<()> {
    let mut reader = BufReader::new(read);
    let mut buf = vec![];

    let i = reader.read_until(0x20, &mut buf)?;
    let obj_ty = ObjType::from_str(std::str::from_utf8(&buf[..i - 1]).unwrap());

    let j = reader.read_until(0x00, &mut buf)?;
    let size = std::str::from_utf8(&buf[i..i + j - 1]).unwrap().parse::<usize>().unwrap();
    let len = reader.read_to_end(&mut buf)?;
    debug_assert_eq!(len, size);
    let contents = &buf[i + j..];
    assert_eq!(contents.len(), size);
    Ok(())
}

impl BitRepo {
    /// format: <type> 0x20 <size> 0x00 <content>
    pub fn read_obj_from_hash(&self, hash: &str) -> BitResult<()> {
        let obj_path = self.relative_paths(&["objects", &hash[0..2], &hash[2..]]);
        let z = ZlibDecoder::new(File::open(obj_path)?);
        read_obj(z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_obj_read() {
        let mut bytes = vec![];
        bytes.extend(b"blob ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xywz");
        read_obj(bytes.as_slice()).unwrap();
    }

    #[test]
    #[should_panic]
    fn invalid_obj_read_wrong_size() {
        let mut bytes = vec![];
        bytes.extend(b"blob ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xyw");

        let _ = read_obj(bytes.as_slice());
    }

    #[test]
    #[should_panic]
    fn invalid_obj_read_unknown_obj_ty() {
        let mut bytes = vec![];
        bytes.extend(b"weirdobjty ");
        bytes.extend(b"12\0");
        bytes.extend(b"abcd1234xywz");

        let _ = read_obj(bytes.as_slice());
    }
}
