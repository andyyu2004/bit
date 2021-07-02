use crate::diff::WorkspaceDiff;
use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use bitflags::bitflags;
use owo_colors::OwoColorize;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, PartialEq)]
pub struct BitStatus {
    head: BitRef,
    flags: BitStatusFlags,
    // TODO can use bitflags if more bools pop up here
    pub staged: WorkspaceDiff,
    pub unstaged: WorkspaceDiff,
}

bitflags! {
    #[derive(Default)]
    pub struct BitStatusFlags: u8 {
        // whether we have no prior commits
        const INITIAL = 1;
    }
}

impl BitStatus {
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty()
    }

    pub fn is_initial(&self) -> bool {
        self.flags.contains(BitStatusFlags::INITIAL)
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn status(self, pathspec: Pathspec) -> BitResult<BitStatus> {
        self.with_index_mut(|index| {
            let head = self.read_head()?;
            let staged = index.diff_head(pathspec)?;
            let unstaged = index.diff_worktree(pathspec)?;

            let is_initial = self.try_fully_resolve_ref(head)?.is_none();

            let mut flags = BitStatusFlags::default();
            flags.set(BitStatusFlags::INITIAL, is_initial);

            Ok(BitStatus { head, staged, unstaged, flags })
        })
    }
}

// TODO if a directory only contains untracked directories
// it should just print the directory and not its contents
impl Display for BitStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.head {
            BitRef::Direct(oid) => writeln!(f, "HEAD detached at `{}`", oid)?,
            BitRef::Symbolic(branch) => writeln!(f, "On branch `{}`", branch)?,
        };

        writeln!(f)?;

        if !self.staged.is_empty() {
            writeln!(f, "Changes to be committed")?;
            writeln!(f, "  (use `bit restore --staged <file>...` to unstage) (unimplemented)")?;
            for new in &self.staged.new {
                writeln!(f, "\t{}:   {}", "new file".green(), new.path.green())?;
            }
            for (_, modified) in &self.staged.modified {
                writeln!(f, "\t{}:   {}", "modified".green(), modified.path.green())?;
            }
            for deleted in &self.staged.deleted {
                writeln!(f, "\t{}:   {}", "deleted".green(), deleted.path.green())?;
            }
            writeln!(f)?;
        }

        if !self.unstaged.modified.is_empty() || !self.unstaged.deleted.is_empty() {
            writeln!(f, "Changes not staged for commit")?;
            writeln!(f, "  (use `bit add <file>...` to update what will be committed)")?;
            writeln!(f, "  (use 'bit restore <file>...' to discard changes in working directory)")?;
            for (_, modified) in &self.unstaged.modified {
                writeln!(f, "\t{}:   {}", "modified".red(), modified.path.red())?;
            }
            for deleted in &self.unstaged.deleted {
                writeln!(f, "\t{}:   {}", "deleted".red(), deleted.path.red())?;
            }
            writeln!(f)?;
        }

        if !self.unstaged.new.is_empty() {
            writeln!(f, "Untracked files:")?;
            writeln!(f, "  (use `bit add <file>...` to include in what will be committed)")?;
            for untracked in &self.unstaged.new {
                writeln!(f, "\t{}", untracked.path.red())?;
            }
        }

        if !self.unstaged.is_empty() && self.staged.is_empty() {
            writeln!(f, "no changes added to commit (use `bit add`) to stage")?;
        } else if self.is_empty() {
            if self.is_initial() {
                writeln!(f, "nothing to commit (create/copy files and use `bit add` to track)")?;
            } else if !self.unstaged.new.is_empty() {
                writeln!(
                    f,
                    "nothing added to commit but untracked files present (use `git add` to track)"
                )?;
            } else {
                writeln!(f, "nothing to commit, working tree clean")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
