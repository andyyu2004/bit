use crate::diff::{HeadIndexDiff, IndexWorktreeDiff};
use crate::error::BitResult;
use crate::path::BitPath;
use crate::repo::BitRepo;
use colored::*;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct BitStatusReport {
    staged: Vec<BitPath>,
    deleted: Vec<BitPath>,
    untracked: Vec<BitPath>,
    modified: Vec<BitPath>,
    new: Vec<BitPath>,
}

impl BitRepo {
    pub fn status_report(&self) -> BitResult<BitStatusReport> {
        let IndexWorktreeDiff { untracked, modified } = self.diff_index_worktree()?;
        let HeadIndexDiff { deleted, staged, new } = self.diff_head_index()?;
        Ok(BitStatusReport { untracked, modified, staged, new, deleted })
    }
}

// TODO if a directory only contains untracked directories
// it should just print the directory and not its contents
impl Display for BitStatusReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !self.staged.is_empty() || !self.new.is_empty() {
            writeln!(f, "Changes to be committed")?;
            writeln!(f, "  (use `bit restore --staged <file>...` to unstage) (unimplemented)")?;
            for path in &self.new {
                writeln!(f, "\t{}:   {}", "new file".green(), path.green())?;
            }
            for path in &self.staged {
                writeln!(f, "\t{}:   {}", "modified".green(), path.green())?;
            }
            writeln!(f)?;
        }
        if !(self.modified.is_empty()) {
            writeln!(f, "Changes not staged for commit")?;
            writeln!(f, "  (use `bit add <file>...` to update what will be committed)")?;
            writeln!(f, "  (use 'bit restore <file>...' to discard changes in working directory)")?;
            for path in &self.modified {
                writeln!(f, "\t{}:   {}", "modified".red(), path.red())?;
            }
            writeln!(f)?;
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
