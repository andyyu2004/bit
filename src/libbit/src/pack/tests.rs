use lazy_static::lazy_static;

use super::*;
use std::io::Cursor;
use std::str::FromStr;

// got this number by inspecting last entry of the fanout table
const PACK_LEN: u64 = 11076;

#[test]
fn test_deserialize_pack_idx_is_ok() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/pack.idx") as &[u8];
    let _pack_idx = PackIndex::deserialize_unbuffered(bytes)?;
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_start() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let index = PackIndexReader::new(&mut cursor)?.find_oid_index(
        // this hash is the first oid in sorted list
        BitHash::from_str("0004a3cf85dbcbfbef916599145a0c370bb78cf5").unwrap(),
    )?;
    assert_eq!(index, 0);
    Ok(())
}
#[test]
fn test_pack_idx_find_oid_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let index = PackIndexReader::new(&mut cursor)?.find_oid_index(
        // this hash is the last oid in sorted list
        BitHash::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    assert_eq!(index, PACK_LEN - 1);
    Ok(())
}

fn pack() -> Pack {
    Pack {
        pack: BitPath::intern("tests/files/pack.pack"),
        idx: BitPath::intern("tests/files/pack.idx"),
    }
}

lazy_static! {
    static ref OID: BitHash = "1806658f16f76480a3f40461db577a02d1e01591".parse().unwrap();
}

#[test]
fn test_check_oid_exists_in_pack() -> BitResult<()> {
    assert!(pack().obj_exists(*OID)?);
    Ok(())
}

#[test]
fn test_find_offset_in_pack() -> BitResult<()> {
    let (_crc, offset) = pack().index_reader()?.find_oid_crc_offset(*OID)?;
    assert_eq!(offset, 2247656);
    Ok(())
}

#[test]
fn test_read_type_and_size_from_offset_in_pack() -> BitResult<()> {
    let pack = pack();
    let (_crc, offset) = pack.index_reader()?.find_oid_crc_offset(*OID)?;
    let (obj_ty, size) = pack.pack_reader()?.read_header_from_offset(offset)?;
    assert_eq!(obj_ty, BitObjType::Commit);
    assert_eq!(size, 215);
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_offset_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let (_crc, pack_idx) = PackIndexReader::new(&mut cursor)?.find_oid_crc_offset(
        // this hash is the last oid in sorted list
        BitHash::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    // `git verify-pack src/libbit/tests/files/pack.pack -v | rg fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47`
    assert_eq!(pack_idx, 2151306);
    Ok(())
}
