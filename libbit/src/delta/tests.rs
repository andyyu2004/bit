use super::*;

#[test]
fn test_build_delta_index_multiple() {
    let bytes = b"the quick brown fox jumps over the slow lazy dog";
    let delta_index = DeltaIndex::new(bytes);
    let expected = hashmap! {
        b"the quick brown " => 0,
        b"fox jumps over t" => 16,
        b"he slow lazy dog" => 32,
    };
    assert_eq!(delta_index.indices, expected);
}

#[test]
fn test_build_delta_index_non_divisor_should_ignore_partial_chunk() {
    let bytes = b"the quick brown fox jumps over the lazy dog";
    let delta_index = DeltaIndex::new(bytes);
    let expected = hashmap! {
        b"the quick brown " => 0,
        b"fox jumps over t" => 16,
    };
    assert_eq!(delta_index.indices, expected);
}

#[test]
fn test_parse_delta_insert_op() -> BitResult<()> {
    let op = DeltaOp::deserialize(&mut &[0x3, 0xa, 0xb, 0xc][..])?;
    assert_eq!(op, DeltaOp::Insert(vec![0xa, 0xb, 0xc]));
    Ok(())
}

#[test]
fn test_parse_delta_copy_op() -> BitResult<()> {
    let op =
        DeltaOp::deserialize(&mut &[0b11111111, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd][..])?;
    // little endian encoding
    assert_eq!(op, DeltaOp::Copy(0x67452301, 0xcdab89));
    Ok(())
}

#[test]
fn test_parse_delta_zero_size() -> BitResult<()> {
    let op = DeltaOp::deserialize(&mut &[0x80][..])?;
    assert_eq!(op, DeltaOp::Copy(0, 0x10000));
    Ok(())
}

// #[test]
// fn test_delta_compress_simple_outputs_correct_operations() {
//     let source = b"the quick brown fox jumps over the slow lazy dog";
//     let target = b"over the slow lazy dog the quick brown fox jumps";
//     let ops = DeltaIndex::new(source).compress(target);
//     dbg!(ops);
// }
