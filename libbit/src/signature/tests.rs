use super::*;
use crate::test_utils::generate_sane_string_with_newlines;
use quickcheck::{quickcheck, Arbitrary, Gen};
use rand::Rng;

impl Arbitrary for BitSignature {
    fn arbitrary(_g: &mut Gen) -> Self {
        Self {
            name: generate_sane_string_with_newlines(5..100),
            email: generate_sane_string_with_newlines(10..200),
            // don't care too much about this being random
            time: BitTime { time: BitEpochTime(12345678), offset: BitTimeZoneOffset(200) },
        }
    }
}

#[test]
fn parse_timezone_offset() {
    let offset = BitTimeZoneOffset::from_str("+0200").unwrap();
    assert_eq!(offset.0, 120);
    let offset = BitTimeZoneOffset::from_str("+1300").unwrap();
    assert_eq!(offset.0, 780);
    let offset = BitTimeZoneOffset::from_str("-0830").unwrap();
    assert_eq!(offset.0, -510);
}

impl Arbitrary for BitTimeZoneOffset {
    fn arbitrary(_g: &mut quickcheck::Gen) -> Self {
        // how to bound quickchecks genrange?
        Self(rand::thread_rng().gen_range(-1000..1000))
    }
}

#[quickcheck(sizee)]
fn serialize_then_parse_timezone(offset: BitTimeZoneOffset) {
    let parsed: BitTimeZoneOffset = offset.to_string().parse().unwrap();
    assert_eq!(offset, parsed)
}

#[test]
fn parse_bit_signature() {
    let sig = "Andy Yu <andyyu2004@gmail.com> 1616061862 +1300";
    let sig = sig.parse::<BitSignature>().unwrap();
    assert_eq!(sig.name, "Andy Yu");
    assert_eq!(sig.email, "andyyu2004@gmail.com");
    assert_eq!(
        sig.time,
        BitTime { time: BitEpochTime(1616061862), offset: BitTimeZoneOffset(780) }
    );
}

#[quickcheck]
fn serialize_then_parse_bit_signature(sig: BitSignature) {
    assert_eq!(sig, sig.to_string().parse().unwrap())
}

#[test]
fn serialize_timezone_offset() {
    let offset = BitTimeZoneOffset(780);
    assert_eq!(format!("{}", offset), "+1300");
    let offset = BitTimeZoneOffset(-200);
    assert_eq!(format!("{}", offset), "-0320");
}
