use crate::diff::WorkspaceStatus;
use crate::error::BitResult;
use crate::index::{BitIndexEntry, Conflict, ConflictType, Conflicts};
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use bitflags::bitflags;
use owo_colors::OwoColorize;
use std::fmt::{self, Display, Formatter};
use std::iter::Peekable;

#[derive(Debug, PartialEq)]
pub struct BitStatus {
    head: BitRef,
    flags: BitStatusFlags,
    // TODO can use bitflags if more bools pop up here
    pub staged: WorkspaceStatus,
    pub unstaged: WorkspaceStatus,
    pub conflicted: Conflicts,
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
            let conflicted = index.conflicts();

            let is_initial = self.try_fully_resolve_ref(head)?.is_none();

            let mut flags = BitStatusFlags::default();
            flags.set(BitStatusFlags::INITIAL, is_initial);

            Ok(BitStatus { head, staged, unstaged, conflicted, flags })
        })
    }
}

// TODO if a directory only contains untracked directories
// it should just print the directory and not its contents
impl Display for BitStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.head {
            BitRef::Direct(oid) => writeln!(f, "HEAD detached at `{}`", oid)?,
            BitRef::Symbolic(branch) => writeln!(f, "On branch `{}`", branch.short())?,
        };

        self.fmt_state(f)?;
        self.fmt_staged(f)?;
        self.fmt_unstaged(f)?;
        self.fmt_unmerged(f)?;
        self.fmt_summary(f)?;

        writeln!(f)?;

        Ok(())
    }
}

fn filter_unmerged<'a>(
    iter: impl IntoIterator<Item = &'a BitIndexEntry>,
) -> Peekable<impl Iterator<Item = &'a BitIndexEntry>> {
    iter.into_iter().filter(|entry| !entry.is_unmerged()).peekable()
}

trait PeekableIsEmpty {
    fn is_empty(&mut self) -> bool;
}

impl<I> PeekableIsEmpty for Peekable<I>
where
    I: Iterator,
{
    fn is_empty(&mut self) -> bool {
        self.peek().is_none()
    }
}

impl BitStatus {
    fn fmt_state(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !self.conflicted.is_empty() {
            writeln!(f, "You have unmerged paths")?;
            writeln!(f, "  (fix conflicts and run `bit commit`)")?;
            writeln!(f, "  (use `git merge --abort` to abort the merge)")?;
        }
        Ok(())
    }

    fn fmt_staged(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut new = filter_unmerged(&self.staged.new);
        let mut modified =
            self.staged.modified.iter().filter(|(old, _)| !old.is_unmerged()).peekable();
        let mut deleted = filter_unmerged(&self.staged.deleted);

        if new.is_empty() && modified.is_empty() && deleted.is_empty() {
            return Ok(());
        }

        writeln!(f, "Changes to be committed")?;
        writeln!(f, "  (use `bit restore --staged <file>...` to unstage) (unimplemented)")?;

        for entry in new {
            writeln!(f, "\t{}:   {}", "new file".green(), entry.path.green())?;
        }

        for (_, entry) in modified {
            if !entry.is_unmerged() {
                writeln!(f, "\t{}:   {}", "modified".green(), entry.path.green())?;
            }
        }

        for entry in deleted {
            writeln!(f, "\t{}:   {}", "deleted".green(), entry.path.green())?;
        }

        Ok(())
    }

    fn fmt_unstaged(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // we filter by `old` here as that is the index entry with the relevant merge stage
        let mut modified =
            self.unstaged.modified.iter().filter(|(old, _)| !old.is_unmerged()).peekable();
        let mut deleted = filter_unmerged(&self.unstaged.deleted);

        if !modified.is_empty() || !deleted.is_empty() {
            writeln!(f)?;
            writeln!(f, "Changes not staged for commit")?;
            writeln!(f, "  (use `bit add <file>...` to update what will be committed)")?;
            writeln!(f, "  (use 'bit restore <file>...' to discard changes in working directory)")?;

            for (_, entry) in modified {
                writeln!(f, "\t{}:   {}", "modified".red(), entry.path.red())?;
            }

            for entry in deleted {
                writeln!(f, "\t{}:   {}", "deleted".red(), entry.path.red())?;
            }
        }

        let mut untracked = filter_unmerged(&self.unstaged.new);
        if !untracked.is_empty() {
            writeln!(f)?;
            writeln!(f, "Untracked files:")?;
            writeln!(f, "  (use `bit add <file>...` to include in what will be committed)")?;
            for entry in untracked {
                writeln!(f, "\t{}", entry.path.red())?;
            }
        }

        Ok(())
    }

    fn fmt_unmerged(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !self.conflicted.is_empty() {
            writeln!(f)?;
            writeln!(f, "Unmerged paths:")?;
            writeln!(f, "  (use `bit add <file>...` to mark resolution)")?;
            for conflict in &self.conflicted {
                writeln!(f, "\t{}", conflict.red())?;
            }
        }
        Ok(())
    }

    fn fmt_summary(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // print status summary
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

impl Display for Conflict {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:\t{}", self.conflict_type, self.path)
    }
}

impl Display for ConflictType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ConflictType::BothModified => "both modified",
            ConflictType::BothAdded => "both added",
            ConflictType::ModifyDelete => "deleted by them",
            ConflictType::DeleteModify => "deleted by us",
        };
        write!(f, "{}", msg)
    }
}

#[cfg(test)]
mod tests;
