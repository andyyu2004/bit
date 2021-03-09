use std::path::PathBuf;

pub(crate) fn path_from_bytes(bytes: impl AsRef<[u8]>) -> PathBuf {
    PathBuf::from(String::from(std::str::from_utf8(bytes.as_ref()).unwrap()))
}

