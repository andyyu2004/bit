use crate::diff::{Differ, GenericDiff};
use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry};
use crate::path::BitPath;
use crate::repo::BitRepo;
use colored::*;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct BitStatusReport {
    untracked: Vec<BitPath>,
    modified: Vec<BitPath>,
}

impl BitRepo {
    pub fn status_report(&self) -> BitResult<BitStatusReport> {
        let WorktreeIndexDiff { untracked, modified } = self.worktree_index_diff()?;
        Ok(BitStatusReport { untracked, modified })
    }

    pub fn worktree_index_diff(&self) -> BitResult<WorktreeIndexDiff> {
        self.with_index_mut(|index| WorktreeIndexDiffer::new(index).run_diff())
    }

    pub fn untracked_files(&self) -> BitResult<Vec<BitPath>> {
        self.worktree_index_diff().map(|diff| diff.untracked)
    }
}

#[derive(Debug)]
pub struct WorktreeIndexDiff {
    untracked: Vec<BitPath>,
    modified: Vec<BitPath>,
}

pub(crate) struct WorktreeIndexDiffer<'a, 'r> {
    repo: &'r BitRepo,
    index: &'a mut BitIndex<'r>,
    untracked: Vec<BitPath>,
    modified: Vec<BitPath>,
    // directories that only contain untracked files
    _untracked_dirs: HashSet<BitPath>,
}

impl<'a, 'r> WorktreeIndexDiffer<'a, 'r> {
    pub fn new(index: &'a mut BitIndex<'r>) -> Self {
        let repo = index.repo;
        Self {
            index,
            repo,
            untracked: Default::default(),
            modified: Default::default(),
            _untracked_dirs: Default::default(),
        }
    }

    fn run_diff(mut self) -> BitResult<WorktreeIndexDiff> {
        let repo = self.repo;
        let index_iter = self.index.iter();
        GenericDiff::run(&mut self, index_iter, repo.worktree_iter()?)?;
        Ok(WorktreeIndexDiff { untracked: self.untracked, modified: self.modified })
    }
}

impl<'a, 'r> Differ<'r> for WorktreeIndexDiffer<'a, 'r> {
    fn index_mut(&mut self) -> &mut BitIndex<'r> {
        self.index
    }

    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        self.untracked.push(new.filepath);
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        assert_eq!(old.filepath, new.filepath);
        Ok(self.modified.push(new.filepath))
    }
}

// TODO if a directory only contains untracked directories
// it should just print the directory and not its contents
impl Display for BitStatusReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !(self.modified.is_empty()) {
            writeln!(f, "Changes not staged for commit")?;
            writeln!(f, "  (use `bit add <file>...` to update what will be committed)")?;
            writeln!(f, "  (use 'bit restore <file>...' to discard changes in working directory)")?;
            for path in &self.modified {
                writeln!(f, "\t{}:   {}", "modified".red(), path.red())?;
            }
        }
        if !self.untracked.is_empty() {
            writeln!(f, "Untracked files:")?;
            writeln!(f, "  (use `bit add <file>...` to include in what will be committed)")?;
            for path in &self.untracked {
                writeln!(f, "\t{}", path.red())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
