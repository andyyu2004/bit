use crate::obj::Oid;
use crate::path::BitPath;
use std::collections::BTreeSet;

use super::*;
use quickcheck_macros::quickcheck;

#[test]
fn test_tree_entry_ordering() {
    let mut entries = BTreeSet::new();
    let dir = TreeEntry { mode: FileMode::DIR, path: BitPath::intern("bar"), hash: Oid::UNKNOWN };
    let file =
        TreeEntry { mode: FileMode::DIR, path: BitPath::intern("bar.ext"), hash: Oid::UNKNOWN };
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
    // 0x30 is utf-8 for "0"
    assert_eq!(hash[5..], [0x30; 35]);
    Ok(())
}

#[test]
fn test_convert_partial_oid_to_oid() -> BitResult<()> {
    let partial = PartialOid::from_str("abcde")?;
    let oid = partial.into_oid()?;
    let mut expected = [0; 20];
    expected[0] = 0xab;
    expected[1] = 0xcd;
    expected[2] = 0xe0;
    assert_eq!(oid.as_bytes(), &expected);
    Ok(())
}

#[test]
fn test_match_oid_prefix() -> BitResult<()> {
    let partial = PartialOid::from_str("abcde")?;
    let mut bytes = [0; 20];
    bytes[0] = 0xab;
    bytes[1] = 0xcd;
    bytes[2] = 0xe0;

    let oid = Oid::new(bytes);
    assert!(oid.has_prefix(partial)?);

    // check it still works even though only half the byte matches
    bytes[2] = 0xe8;
    let oid = Oid::new(bytes);
    assert!(oid.has_prefix(partial)?);

    let partial = PartialOid::from_str("abcded")?;
    assert!(!oid.has_prefix(partial)?);
    Ok(())
}

#[test]
fn test_oid_prefix2() -> BitResult<()> {
    let prefix = PartialOid::from_str("3febc6e")?;
    let oid = Oid::from_str("3febc6e6f3075ed6b2170dc5fd88878a27012b1d")?;
    assert!(oid.has_prefix(prefix)?);
    Ok(())
}
