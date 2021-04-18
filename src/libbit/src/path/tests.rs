use super::*;

#[test]
fn test_path_components() {
    let path = BitPath::intern("foo/bar/baz");
    let components = path.components();
    assert_eq!(components[0], "foo");
    assert_eq!(components[1], "bar");
    assert_eq!(components[2], "baz");
}

#[test]
fn test_path_accumulative_components() {
    let path = BitPath::intern("foo/bar/baz");
    let mut components = path.accumulative_components();
    assert_eq!(components.next().unwrap(), "foo");
    assert_eq!(components.next().unwrap(), "foo/bar");
    assert_eq!(components.next().unwrap(), "foo/bar/baz");
}
