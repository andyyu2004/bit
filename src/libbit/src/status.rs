use crate::diff::{HeadIndexDiff, IndexWorktreeDiff};
use crate::error::BitResult;
use crate::repo::BitRepo;
use colored::*;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct BitStatusReport {
    pub staged: HeadIndexDiff,
    pub unstaged: IndexWorktreeDiff,
}

impl BitRepo {
    pub fn status_report(&self) -> BitResult<BitStatusReport> {
        // TODO rename diff to status as we need the term diff for other matters
        let unstaged = self.diff_index_worktree()?;
        let staged = self.diff_head_index()?;
        Ok(BitStatusReport { staged, unstaged })
    }
}

// TODO if a directory only contains untracked directories
// it should just print the directory and not its contents
impl Display for BitStatusReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !self.staged.is_empty() {
            writeln!(f, "Changes to be committed")?;
            writeln!(f, "  (use `bit restore --staged <file>...` to unstage) (unimplemented)")?;
            for new in &self.staged.new {
                writeln!(f, "\t{}:   {}", "new file".green(), new.filepath.green())?;
            }
            for (_, modified) in &self.staged.modified {
                writeln!(f, "\t{}:   {}", "modified".green(), modified.filepath.green())?;
            }
            for deleted in &self.staged.deleted {
                writeln!(f, "\t{}:   {}", "deleted".green(), deleted.filepath.green())?;
            }
            writeln!(f)?;
        }

        if !self.unstaged.modified.is_empty() || !self.unstaged.deleted.is_empty() {
            writeln!(f, "Changes not staged for commit")?;
            writeln!(f, "  (use `bit add <file>...` to update what will be committed)")?;
            writeln!(f, "  (use 'bit restore <file>...' to discard changes in working directory)")?;
            for (_, modified) in &self.unstaged.modified {
                writeln!(f, "\t{}:   {}", "modified".red(), modified.filepath.red())?;
            }
            for deleted in &self.unstaged.deleted {
                writeln!(f, "\t{}:   {}", "deleted".red(), deleted.filepath.red())?;
            }
            writeln!(f)?;
        }

        if !self.unstaged.untracked.is_empty() {
            writeln!(f, "Untracked files:")?;
            writeln!(f, "  (use `bit add <file>...` to include in what will be committed)")?;
            for untracked in &self.unstaged.untracked {
                writeln!(f, "\t{}", untracked.filepath.red())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
