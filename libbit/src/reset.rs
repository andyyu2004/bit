use crate::error::BitResult;
use crate::peel::Peel;
use crate::refs::{BitRef, SymbolicRef};
use crate::repo::{BitRepo, RepoState};
use crate::rev::Revspec;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum ResetKind {
    Soft,
    Mixed,
    Hard,
}

impl Default for ResetKind {
    fn default() -> Self {
        ResetKind::Mixed
    }
}

impl<'rcx> BitRepo<'rcx> {
    /// Set the current branch to point at the specified commit_oid `target`
    /// and set the working tree/index to match depending on the reset kind.
    /// [ResetKind::Soft] does only the above.
    /// [ResetKind::Mixed] does a `soft` reset and also makes the index match the target tree
    /// [ResetKind::Hard] does a `mixed` reset and the working tree will match the target tree
    pub fn reset(self, revision: &Revspec, kind: ResetKind) -> BitResult<()> {
        let target = self.resolve_rev(revision)?;
        if self.repo_state() == RepoState::Merging {
            bail!("cannot perform reset when repository is in the middle of a merge")
        }

        let target_commit_oid = self.fully_resolve_ref(target)?;
        let target_commit = target_commit_oid.peel(self)?;
        let tree = target_commit.tree_oid();

        // Important to call `checkout_tree` before HEAD is updated as it internally read's the current head.
        // This should probably change once checkout_tree takes some options which should explicitly include the baseline tree
        // Also, do the `checkout` before the index `read_tree` as `checkout` will touch the index too,
        // but we want to `read_tree` to have the final say on the state of the index.
        if kind > ResetKind::Mixed {
            // force checkout the tree, `checkout_tree` is always a force checkout for now
            self.checkout_tree(tree)?;
        }

        if kind > ResetKind::Soft {
            // make index match the target commit's tree
            self.with_index_mut(|index| index.read_tree(tree))?;
        }

        // Move the current branch to the target
        // If we are current in detached head state, then we move HEAD to the target reference (either direct or indirect)
        // If we are on a branch, then we move that branch to the target oid directly (i.e. we don't want our branch to point at another branch)
        match self.read_head()? {
            BitRef::Direct(..) => self.update_ref_for_reset(SymbolicRef::HEAD, target)?,
            BitRef::Symbolic(current_branch) =>
                self.update_ref_for_reset(current_branch, target_commit_oid)?,
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests;
