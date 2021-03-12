use crate::path::BitPath;

pub(crate) fn path_from_bytes(bytes: impl AsRef<[u8]>) -> BitPath {
    BitPath::from(std::str::from_utf8(bytes.as_ref()).unwrap())
}
