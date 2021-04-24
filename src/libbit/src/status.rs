use crate::diff::{Differ, GenericDiff};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::path::BitPath;
use crate::repo::BitRepo;
use colored::*;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct BitStatusReport {
    untracked: Vec<BitPath>,
}

impl BitRepo {
    pub fn status_report(&self) -> BitResult<BitStatusReport> {
        let untracked = UntrackedBuilder::new(self).get_untracked()?;
        Ok(BitStatusReport { untracked })
    }
}

struct UntrackedBuilder<'r> {
    repo: &'r BitRepo,
    untracked: Vec<BitPath>,
}

impl<'r> UntrackedBuilder<'r> {
    pub fn new(repo: &'r BitRepo) -> Self {
        Self { repo, untracked: Default::default() }
    }

    fn get_untracked(mut self) -> BitResult<Vec<BitPath>> {
        let repo = self.repo;
        repo.with_index(|index| GenericDiff::run(&mut self, index.iter(), repo.worktree_iter()?))?;
        Ok(self.untracked)
    }
}

impl Differ for UntrackedBuilder<'_> {
    fn on_create(&mut self, new: BitIndexEntry) -> BitResult<()> {
        println!("create {}", new.filepath);
        self.untracked.push(new.filepath);
        Ok(())
    }

    fn on_update(&mut self, _old: BitIndexEntry, _new: BitIndexEntry) -> BitResult<()> {
        Ok(())
    }

    fn on_delete(&mut self, _old: BitIndexEntry) -> BitResult<()> {
        println!("delete {}", _old.filepath);
        Ok(())
    }
}

impl Display for BitStatusReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "untracked files:")?;
        writeln!(f, "  (use `bit add <file>...` to include in what will be committed)")?;
        for path in &self.untracked {
            writeln!(f, "\t{}", path.as_str().red())?;
        }
        Ok(())
    }
}
