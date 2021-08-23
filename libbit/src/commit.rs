use crate::diff::WorkspaceStatus;
use crate::error::BitResult;
use crate::obj::{BitObject, Commit, Oid};
use crate::pathspec::Pathspec;
use crate::peel::Peel;
use crate::refs::{BitRef, RefUpdateCause, RefUpdateCommitKind, SymbolicRef};
use crate::repo::BitRepo;
use enumflags2::bitflags;
use std::fmt::{self, Display, Formatter};

#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
enum BitStatusFlags {
    Initial,
}

impl<'rcx> BitRepo<'rcx> {
    // TODO return a BitCommitReport which includes the oid, and kind (CommitKind) etc
    pub fn commit(self, msg: Option<String>) -> BitResult<CommitSummary<'rcx>> {
        let head = self.read_head()?;
        let sym = match head {
            BitRef::Direct(..) => SymbolicRef::HEAD,
            BitRef::Symbolic(sym) => sym,
        };
        let parent = self.try_fully_resolve_ref(sym)?;

        let tree = self.write_tree()?;
        let head_tree = self.head_tree()?;

        // don't allow empty commits; also don't currently provide the option to do so as it's not that useful
        // the rhs of the disjunction checks for the case of an empty initial commit
        if tree == head_tree || head_tree.is_unknown() && tree == Oid::EMPTY_TREE {
            // rather oddly, we bail with the status report as the error message
            bail!(self.status(Pathspec::MATCH_ALL)?)
        }

        let commit_oid = self.commit_tree(parent, msg, tree)?;
        let commit = commit_oid.peel(self)?;

        // TODO print status of commit
        // include initial commit if it is one
        // probably amend too (check with git)
        let cause = RefUpdateCause::Commit {
            subject: commit.message.subject.to_owned(),
            kind: if head_tree.is_known() {
                RefUpdateCommitKind::Normal
            } else {
                RefUpdateCommitKind::Initial
            },
        };

        self.update_ref(sym, commit_oid, cause)?;

        Ok(CommitSummary {
            status: self.diff_tree_to_tree(parent.unwrap_or(Oid::UNKNOWN), commit.tree)?,
            sym,
            commit,
        })
    }
}

#[derive(Debug)]
pub struct CommitSummary<'rcx> {
    /// the symbolic reference that was moved by this commit
    pub sym: SymbolicRef,
    /// the newly created commit object
    pub commit: Commit<'rcx>,
    /// the difference between HEAD^ and HEAD
    pub status: WorkspaceStatus,
}

impl Display for CommitSummary<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.sym == SymbolicRef::HEAD {
            writeln!(f, "[detached HEAD {}]", self.commit.oid())?;
        } else {
            writeln!(f, "[{} {}]", self.sym.short(), self.commit.oid())?;
        }

        // TODO show full diffstat summary (deletions, insertions)
        let files_changed = self.status.len();
        writeln!(f, "{} file{} changed", files_changed, pluralize!(files_changed))?;

        for created in &self.status.new {
            writeln!(f, "create mode {} {}", created.mode, created.path)?;
        }

        for deleted in &self.status.deleted {
            writeln!(f, "delete mode {} {}", deleted.mode, deleted.path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
