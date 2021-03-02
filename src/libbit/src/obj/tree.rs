use crate::error::BitResult;
use crate::hash::SHA1Hash;
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub struct FileMode([u8; 6]);

impl AsRef<[u8]> for FileMode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug)]
pub struct Tree {
    entries: HashMap<SHA1Hash, TreeEntry>,
}

#[derive(Debug)]
pub struct TreeEntry {
    mode: FileMode,
    path: PathBuf,
    hash: SHA1Hash,
}

impl TreeEntry {
    pub fn serialize<W: Write>(&self, w: &mut W) -> BitResult<()> {
        for &byte in self.mode.as_ref() {
            write!(w, "{}", byte.to_ascii_lowercase())?;
        }
        Ok(())
        // write!(w, "{}", self.mode)
    }
}
