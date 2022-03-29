use crate::diff::WorkspaceStatus;
use crate::error::BitResult;
use crate::obj::{BitObject, Commit, Oid};
use crate::pathspec::Pathspec;
use crate::peel::Peel;
use crate::refs::{BitRef, RefUpdateCause, RefUpdateCommitKind, SymbolicRef};
use crate::repo::BitRepo;
use crate::xdiff::DiffFormatExt;
use enumflags2::bitflags;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

#[bitflags]
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum BitStatusFlags {
    Initial,
}

#[derive(Debug, Default)]
pub struct CommitOpts {
    pub message: Option<String>,
    pub allow_empty: bool,
}

impl BitRepo {
    // TODO return a BitCommitReport which includes the oid, and kind (CommitKind) etc
    pub fn commit(self, opts: CommitOpts) -> BitResult<CommitSummary> {
        let head = self.read_head()?;
        let sym = match head {
            BitRef::Direct(..) => SymbolicRef::HEAD,
            BitRef::Symbolic(sym) => sym,
        };
        let parent = self.try_fully_resolve_ref(sym)?;

        let tree = self.write_tree()?;
        let head_tree = self.head_tree()?;

        // The RHS of the disjunction checks for the case of an empty initial commit
        let commit_is_empty =
            tree == head_tree || head_tree.is_unknown() && tree == Oid::EMPTY_TREE;
        if !opts.allow_empty && commit_is_empty {
            // rather oddly, we bail with the status report as the error message
            bail!(self.status(Pathspec::MATCH_ALL)?)
        }

        let commit_oid = self.commit_tree(tree, parent.into_iter().collect(), opts.message)?;
        let commit = commit_oid.peel(&self)?;

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
            repo: self,
            sym,
            commit,
        })
    }
}

#[derive(Debug)]
pub struct CommitSummary {
    pub repo: BitRepo,
    /// the symbolic reference that was moved by this commit
    pub sym: SymbolicRef,
    /// the newly created commit object
    pub commit: Arc<Commit>,
    /// the difference between HEAD^ and HEAD
    pub status: WorkspaceStatus,
}

impl Display for CommitSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.sym == SymbolicRef::HEAD {
            writeln!(f, "[detached HEAD {}]", self.commit.oid().short())?;
        } else {
            writeln!(f, "[{} {}]", self.sym.short(), self.commit.oid().short())?;
        }

        // how to deal with error handling in display impls?
        let _ = self.status.print_diffstat(&self.repo);
        let _ = self.status.print_change_summary();

        Ok(())
    }
}

#[cfg(test)]
mod tests;
