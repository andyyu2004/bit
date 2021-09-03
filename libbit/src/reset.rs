use crate::checkout::CheckoutOpts;
use crate::error::BitResult;
use crate::index::BitIndex;
use crate::peel::Peel;
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
    pub fn reset(self, revision: &Revspec, kind: ResetKind) -> BitResult<()> {
        self.with_index_mut(|index| index.reset(revision, kind))
    }
}

impl<'rcx> BitIndex<'rcx> {
    /// Set the current branch to point at the specified commit_oid `target`
    /// and set the working tree/index to match depending on the reset kind.
    /// [ResetKind::Soft] does only the above.
    /// [ResetKind::Mixed] does a `soft` reset and also makes the index match the target tree
    /// [ResetKind::Hard] does a `mixed` reset and the working tree will match the target tree
    pub fn reset(&mut self, revision: &Revspec, kind: ResetKind) -> BitResult<()> {
        let repo = self.repo;
        let target = repo.resolve_rev(revision)?;
        if repo.repo_state() == RepoState::Merging {
            bail!("cannot perform reset when repository is in the middle of a merge")
        }

        let target_commit_oid = repo.fully_resolve_ref(target)?;
        let target_commit = target_commit_oid.peel(repo)?;
        let target_tree = target_commit.tree_oid();

        // Important to call `checkout_tree` before HEAD is updated as it internally read's the current head.
        // This should probably change once checkout_tree takes some options which should explicitly include the baseline tree
        // Also, do the `checkout` before the index `read_tree` as `checkout` will touch the index too,
        // but we want to `read_tree` to have the final say on the state of the index.
        // in fact, checkout_tree should imply read_tree. Checkout needs some work :)
        if kind > ResetKind::Mixed {
            // force checkout the tree
            self.force_checkout_tree(target_tree)?;
        }

        if kind > ResetKind::Soft {
            // make index match the target commit's tree
            self.read_tree(target_tree)?;
        }

        repo.update_current_ref_for_reset(target_commit_oid)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
