use super::*;

#[test]
fn test_deserialize_pack_idx() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/pack.idx") as &[u8];
    let pack_idx = PackIndex::deserialize_unbuffered(bytes)?;
    // dbg!(pack_idx);
    Ok(())
}
