use crate::obj::Oid;
use crate::path::BitPath;
use std::collections::BTreeSet;

use super::*;
use quickcheck_macros::quickcheck;

#[test]
fn test_tree_entry_ordering() {
    let mut entries = BTreeSet::new();
    let dir = TreeEntry { mode: FileMode::DIR, path: BitPath::intern("bar"), hash: Oid::ZERO };
    let file = TreeEntry { mode: FileMode::DIR, path: BitPath::intern("bar.ext"), hash: Oid::ZERO };
    entries.insert(dir);
    entries.insert(file);
    // files come first
    assert_eq!(entries.pop_first().unwrap().path, "bar.ext");
    assert_eq!(entries.pop_first().unwrap().path, "bar");
}

#[test]
fn valid_obj_read() {
    let mut bytes = vec![];
    bytes.extend(b"blob ");
    bytes.extend(b"12\0");
    bytes.extend(b"abcd1234xywz");
    read_obj_unbuffered(bytes.as_slice()).unwrap();
}

#[test]
#[should_panic]
fn invalid_obj_read_wrong_size() {
    let mut bytes = vec![];
    bytes.extend(b"blob ");
    bytes.extend(b"12\0");
    bytes.extend(b"abcd1234xyw");

    let _ = read_obj_unbuffered(bytes.as_slice());
}

#[test]
#[should_panic]
fn invalid_obj_read_unknown_obj_ty() {
    let mut bytes = vec![];
    bytes.extend(b"weirdobjty ");
    bytes.extend(b"12\0");
    bytes.extend(b"abcd1234xywz");

    let _ = read_obj_unbuffered(bytes.as_slice());
}

#[test]
fn write_read_blob_obj() -> BitResult<()> {
    let bit_obj = BitObjKind::Blob(Blob { bytes: b"hello".to_vec() });
    let bytes = bit_obj.serialize_with_headers()?;
    let parsed_bit_obj = read_obj_unbuffered(bytes.as_slice()).unwrap();
    assert_eq!(bit_obj, parsed_bit_obj);
    Ok(())
}

#[quickcheck]
fn read_write_blob_obj_preserves_bytes(bytes: Vec<u8>) -> BitResult<()> {
    let bit_obj = BitObjKind::Blob(Blob { bytes });
    let serialized = bit_obj.serialize_with_headers()?;
    let parsed_bit_obj = read_obj_unbuffered(serialized.as_slice()).unwrap();
    assert_eq!(bit_obj, parsed_bit_obj);
    Ok(())
}

#[test]
fn construct_partial_hash() -> BitResult<()> {
    let hash = PartialOid::from_str("8e37a")?;
    assert_eq!(&hash[0..5], b"8e37a");
    assert_eq!(hash[5..], [0u8; 35]);
    Ok(())
}
