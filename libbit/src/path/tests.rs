use super::*;
use crate::test_utils::generate_random_string;
use quickcheck::Arbitrary;

macro_rules! p {
    ($path:expr) => {
        BitPath::intern($path)
    };
}

impl Arbitrary for BitPath {
    fn arbitrary(_g: &mut quickcheck::Gen) -> Self {
        (0..5).map(|_| p!(generate_random_string(1..10))).fold(BitPath::EMPTY, |acc, x| acc.join(x))
    }
}

#[test]
fn test_path_components() {
    let path = p!("foo/bar/baz");
    let components = path.components();
    assert_eq!(components[0], "foo");
    assert_eq!(components[1], "bar");
    assert_eq!(components[2], "baz");
}

#[test]
fn test_path_accumulative_components() {
    let path = p!("foo/bar/baz");
    let mut components = path.accumulative_components();
    assert_eq!(components.next().unwrap(), "foo");
    assert_eq!(components.next().unwrap(), "foo/bar");
    assert_eq!(components.next().unwrap(), "foo/bar/baz");
}

#[test]
fn test_path_ordering() {
    assert!(p!("foo") < p!("foo/"));
    assert!(p!("foo") < p!("foo/bar"));
    assert!(p!("foo") == p!("foo"));

    assert!(p!("dir/bar.l") < p!("dir/bar/qux"));
}

#[test]
fn test_path_join_empty() {
    assert_ne!(p!("foo").join(""), "foo");
    assert_eq!(p!("foo").join(""), "foo/");
}
