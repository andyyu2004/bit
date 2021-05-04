use super::*;
use crate::hash::SHA1Hash;
use std::io::BufReader;

// checks that hash reader incrementally hashes correctly without the buffer messing stuff up
#[test]
fn test_hash_reader_generates_correct_hash() -> BitResult<()> {
    let original_bytes = include_bytes!("../../tests/files/largeindex") as &[u8];
    let mut reader = BufReader::new(original_bytes);
    let mut hash_reader = HashReader::new_sha1(&mut reader);
    let bytes = hash_reader.read_to_vec()?;
    assert_eq!(bytes, original_bytes);

    // generated using sha1sum
    let expected_hash: [u8; 20] = [
        0xa3, 0x64, 0xf9, 0x22, 0xfe, 0x5d, 0x63, 0x86, 0xe7, 0xb1, 0x2d, 0xb1, 0x24, 0xcb, 0x03,
        0x5c, 0xb5, 0x1a, 0xea, 0xc3,
    ];
    let hash = SHA1Hash::from(hash_reader.finalize_hash());
    assert_eq!(SHA1Hash::new(expected_hash), hash);
    Ok(())
}
