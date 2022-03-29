use crate::error::BitResult;
use crate::repo::BitRepo;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RenameIndex(Option<usize>);

impl RenameIndex {
    pub fn inc(&mut self) {
        match self.0 {
            Some(i) => self.0 = Some(i + 1),
            None => self.0 = Some(0),
        }
    }
}

impl Display for RenameIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(idx) = self.0 {
            write!(f, "_{}", idx)?;
        }
        Ok(())
    }
}

pub struct UniquePath;

impl UniquePath {
    pub fn new(repo: BitRepo, base_path: impl AsRef<Path>) -> BitResult<PathBuf> {
        let base_path = base_path.as_ref();
        let mut i = RenameIndex::default();
        loop {
            let moved_path = PathBuf::from(format!("{}{}", base_path.display(), i));
            if !repo.path_exists(&moved_path)? {
                return Ok(dbg!(moved_path));
            }
            i.inc();
        }
    }
}
