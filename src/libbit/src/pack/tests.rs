use super::*;
use std::io::Cursor;
use std::str::FromStr;

// got this number by inspecting last entry of the fanout table
const PACK_LEN: usize = 11076;

#[test]
fn test_deserialize_pack_idx_is_ok() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/pack.idx") as &[u8];
    let _pack_idx = PackIndex::deserialize_unbuffered(bytes)?;
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_start() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let pack_idx = PackIndex::find_oid_index(
        &mut cursor,
        // this hash is the first oid in sorted list
        BitHash::from_str("0004a3cf85dbcbfbef916599145a0c370bb78cf5").unwrap(),
    )?;
    assert_eq!(pack_idx, 0);
    Ok(())
}
#[test]
fn test_pack_idx_find_oid_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let pack_idx = PackIndex::find_oid_index(
        &mut cursor,
        // this hash is the last oid in sorted list
        BitHash::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    assert_eq!(pack_idx, PACK_LEN - 1);
    Ok(())
}
