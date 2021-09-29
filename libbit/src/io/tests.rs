use super::*;
use crate::hash::SHA1Hash;
use std::io::BufReader;

// checks that hash reader incrementally hashes correctly without the buffer messing stuff up
#[test]
fn test_hash_reader_generates_correct_hash() -> BitResult<()> {
    let original_bytes = include_bytes!("../../tests/files/mediumindex") as &[u8];
    let mut reader = BufReader::new(original_bytes);
    let mut hash_reader = HashReader::new_sha1(reader);
    let bytes = hash_reader.read_to_vec()?;
    assert_eq!(bytes, original_bytes);

    // generated using sha1sum
    let expected_hash: [u8; 20] = [
        0xa3, 0x64, 0xf9, 0x22, 0xfe, 0x5d, 0x63, 0x86, 0xe7, 0xb1, 0x2d, 0xb1, 0x24, 0xcb, 0x03,
        0x5c, 0xb5, 0x1a, 0xea, 0xc3,
    ];
    let hash = hash_reader.finalize_sha1_hash();
    assert_eq!(SHA1Hash::new(expected_hash), hash);
    Ok(())
}

#[test]
fn test_read_le_varint() -> io::Result<()> {
    // 0100 1011
    let mut bytes = &[0x4d][..];
    assert_eq!(bytes.read_le_varint()?, 0x4d);

    // 1100 1001 1000 1101 0111 1010
    // 0xc9      0x8d      0x7a
    let mut bytes = &[0xc9, 0x8d, 0x7a][..];
    // 111 1010 000 1101 100 1001 (to le ignoring msb)
    // 0001 1110 1000 0110 1100 1001
    // 0x1e 0x86 0xc9
    assert_eq!(bytes.read_le_varint()?, 0x1e86c9);

    let mut bytes = &[0b10001101, 0b10001011, 0b01101010][..];
    // 1101010 0001011 0001101
    // 0001 1010 1000 0101 1000 1101
    // 0x1a 0x85 0x8d
    // apprently correct answer is 0x350bd
    assert_eq!(bytes.read_le_varint()?, 0x1a858d);

    Ok(())
}

#[test]
fn test_read_offset() -> io::Result<()> {
    let mut bytes = &[0b10000001, 0b10010000, 0b00100000][..];
    assert_eq!(bytes.read_offset()?, 34976);
    Ok(())
}

#[test]
fn test_read_le_packed_int() -> io::Result<()> {
    let header = 0b11010010;
    let mut bytes = &[0x35, 0x15, 0x82][..];
    assert_eq!(bytes.read_le_packed(header)?, 0x82001500003500);
    Ok(())
}

#[test]
fn test_read_le_packed_header_only() -> io::Result<()> {
    let header = 0b10000000;
    let mut bytes = &[][..];
    assert_eq!(bytes.read_le_packed(header)?, 0);
    Ok(())
}
