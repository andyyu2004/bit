use crate::diff::*;
use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitEntryIterator, BitTreeIterator};
use crate::obj::{FileMode, TreeEntry, Treeish};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use anyhow::Context;
#[allow(unused_imports)]
use fallible_iterator::{FallibleIterator, FallibleLendingIterator};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::prelude::{OsStrExt, PermissionsExt};

#[derive(Debug, Default)]
pub struct CheckoutOpts {
    pub strategy: CheckoutStrategy,
}

impl CheckoutOpts {
    pub fn forced() -> Self {
        Self { strategy: CheckoutStrategy::Force, ..Default::default() }
    }

    pub fn is_force(&self) -> bool {
        self.strategy == CheckoutStrategy::Force
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheckoutStrategy {
    Safe,
    Force,
}

impl Default for CheckoutStrategy {
    fn default() -> Self {
        CheckoutStrategy::Safe
    }
}

impl<'rcx> BitRepo<'rcx> {
    /// checkout the branch/commit specified by the revision
    /// - updates the worktree to match the tree represented by the tree of the commit
    /// - moves HEAD to point at the branch/commit
    pub fn checkout_revision(self, rev: &Revspec, opts: CheckoutOpts) -> BitResult<()> {
        let reference = self.resolve_rev(rev)?;
        self.checkout(reference, opts)
    }

    pub fn checkout(self, reference: impl Into<BitRef>, opts: CheckoutOpts) -> BitResult<()> {
        let reference: BitRef = reference.into();
        // doesn't make sense to move HEAD -> HEAD
        assert_ne!(reference, BitRef::HEAD);

        let status = self.status(Pathspec::MATCH_ALL)?;

        // only allow checkout on fully clean states for now
        ensure!(status.is_empty(), "cannot checkout: unclean state");

        let commit_oid = self.fully_resolve_ref(reference)?;
        self.checkout_tree_with_opts(commit_oid, opts)?;

        // checking out a remote reference should puts us in detached head state
        // as we should not be allowed to modify remote branches locally
        let new_head = if reference.is_remote() { commit_oid.into() } else { reference };
        self.update_head_for_checkout(new_head)?;

        debug_assert!(
            self.status(Pathspec::MATCH_ALL)?.is_empty(),
            "the working tree and index should exactly match"
        );

        Ok(())
    }

    pub fn checkout_tree_with_opts(
        self,
        treeish: impl Treeish<'rcx>,
        opts: CheckoutOpts,
    ) -> BitResult<()> {
        self.with_index_mut(|index| index.checkout_tree_with_opts(treeish, opts))
    }

    pub fn checkout_tree(self, treeish: impl Treeish<'rcx>) -> BitResult<()> {
        self.checkout_tree_with_opts(treeish, CheckoutOpts::default())
    }
}

impl<'rcx> BitIndex<'rcx> {
    /// Update working directory and index to match the tree referenced by `treeish`.
    // NOTE
    // - There are currently no safety checks here! (i.e. it does a force checkout)
    // - Don't update HEAD before calling this as this does a diff relative to the current HEAD (for now)
    pub fn checkout_tree_with_opts(
        &mut self,
        treeish: impl Treeish<'rcx>,
        opts: CheckoutOpts,
    ) -> BitResult<()> {
        let repo = self.repo;
        let target_tree = treeish.treeish_oid(repo)?;
        let baseline = repo.head_tree_iter()?;
        let target = repo.tree_iter(target_tree);
        let worktree = self.worktree_iter()?;
        let migration = self.generate_migration(baseline, target, worktree, opts)?;
        self.apply_migration(&migration)?;
        debug_assert!(self.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
        debug_assert!(self.diff_tree(target_tree, Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    }

    fn generate_migration(
        &mut self,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: impl BitEntryIterator,
        opts: CheckoutOpts,
    ) -> BitResult<Migration> {
        CheckoutCtxt::new(self, opts).generate(baseline, target, worktree)
    }

    fn apply_migration(&mut self, migration: &Migration) -> BitResult<()> {
        let repo = self.repo;
        for rmrf in &migration.rmrfs {
            std::fs::remove_dir_all(repo.to_absolute_path(&rmrf.path))?;
            self.remove_directory(&rmrf.path)?;
        }

        for rm in &migration.rms {
            std::fs::remove_file(repo.to_absolute_path(&rm.path))?;
            self.remove_entry((rm.path, MergeStage::None));
        }

        for mkdir in &migration.mkdirs {
            std::fs::create_dir(repo.to_absolute_path(&mkdir.path))?;
        }

        for create in &migration.creates {
            let path = repo.to_absolute_path(&create.path);
            let blob = create.read_to_blob(repo)?;

            if create.mode.is_link() {
                //? is it guaranteed that a symlink contains the path of the target, or is it fs impl dependent?
                let symlink_target = OsStr::from_bytes(&blob);
                std::os::unix::fs::symlink(symlink_target, path)?;
            } else {
                debug_assert!(create.mode.is_file());
                let mut file = std::fs::File::with_options()
                    .create_new(true)
                    .read(false)
                    .write(true)
                    .open(&path)
                    .with_context(|| anyhow!("failed to create file in migration create"))?;
                file.write_all(&blob)?;
                file.set_permissions(Permissions::from_mode(create.mode.as_u32()))?;
            }

            self.add_entry(BitIndexEntry::from_path(repo, &path)?)?;
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct Migration {
    // recursive deletions of directory and all subentries
    rmrfs: Vec<TreeEntry>,
    // deletion of file
    rms: Vec<TreeEntry>,
    // creation of empty directory
    mkdirs: Vec<TreeEntry>,
    // creation of new file
    creates: Vec<TreeEntry>,
}

impl Migration {
    pub fn generate(
        index: &mut BitIndex<'_>,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: impl BitEntryIterator,
        opts: CheckoutOpts,
    ) -> BitResult<Self> {
        CheckoutCtxt::new(index, opts).generate(baseline, target, worktree)
    }
}
#[derive(Debug)]
enum CheckoutAction {
    None,
    Create,
    Update,
    Delete,
    Conflict,
}

pub struct CheckoutCtxt<'a, 'rcx> {
    repo: BitRepo<'rcx>,
    index: &'a mut BitIndex<'rcx>,
    migration: Migration,
    opts: CheckoutOpts,
}

macro_rules! cond {
    ($pred:expr => $then:ident : $otherwise:ident) => {
        if $pred { CheckoutAction::$then } else { CheckoutAction::$otherwise }
    };
}

impl<'a, 'rcx> CheckoutCtxt<'a, 'rcx> {
    pub fn new(index: &'a mut BitIndex<'rcx>, opts: CheckoutOpts) -> Self {
        let repo = index.repo;
        Self { index, repo, opts, migration: Migration::default() }
    }

    // Refer to https://github.com/libgit2/libgit2/blob/main/docs/checkout-internals.md
    // Following source code more closely than the table though
    pub fn generate(
        mut self,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: impl BitEntryIterator,
    ) -> BitResult<Migration> {
        let mut worktree_iter = worktree.peekable();
        let opts = DiffOpts::with_flags(DiffOptFlags::INCLUDE_UNMODIFIED);
        let diff_iter = self.repo.tree_diff_iter_with_opts(baseline, target, opts);

        // The list of actions to apply and the path to apply it to
        let mut actions: Vec<(TreeEntry, CheckoutAction)> = vec![];
        diff_iter.for_each(|diff_entry| {
            // matchup the worktree iterator with the diff iterator by comparing order of entries
            let action = match worktree_iter.peek()? {
                Some(worktree_entry) => match worktree_entry.entry_cmp(&diff_entry.index_entry()) {
                    // worktree behind diffs, process worktree_entry alone
                    Ordering::Less => {
                        let action = self.action_worktree_only(worktree_entry)?;
                        worktree_iter.next()?;
                        action
                    }
                    // worktree even with diffs, process diff_entry and worktree_entry together
                    Ordering::Equal => {
                        let action = self.action_with_worktree(diff_entry, worktree_entry)?;
                        worktree_iter.next()?;
                        action
                    }
                    // worktree ahead of diffs, process only diff_entry
                    Ordering::Greater => self.action_no_worktree(diff_entry)?,
                },
                // again, worktree ahead of diffs
                None => self.action_no_worktree(diff_entry)?,
            };
            Ok(actions.push(action))
        })?;

        // consume the remaining worktree entries
        worktree_iter.for_each(|worktree_entry| {
            let action = self.action_worktree_only(&worktree_entry)?;
            Ok(actions.push(action))
        })?;

        for (entry, action) in actions {
            match action {
                CheckoutAction::None => {}
                CheckoutAction::Create => self.migration.creates.push(entry),
                CheckoutAction::Delete => self.migration.rms.push(entry),
                CheckoutAction::Update => {
                    self.migration.rms.push(entry);
                    self.migration.creates.push(entry);
                }
                CheckoutAction::Conflict => todo!(),
            }
        }

        Ok(self.migration)
    }

    fn action_worktree_only(
        &mut self,
        worktree_entry: &BitIndexEntry,
    ) -> BitResult<(TreeEntry, CheckoutAction)> {
        Ok((worktree_entry.into(), CheckoutAction::None))
    }

    fn action_with_worktree(
        &mut self,
        diff_entry: TreeDiffEntry<'_>,
        worktree_entry: &BitIndexEntry,
    ) -> BitResult<(TreeEntry, CheckoutAction)> {
        let action = match diff_entry {
            // case 9/10: B1 x B1|B2
            TreeDiffEntry::DeletedBlob(..) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 10: B1 x B2 | delete of modified blob (forceable)
                    cond!(self.opts.is_force() => Delete : Conflict)
                } else {
                    // case 9: B1 x B1 | delete blob (safe)
                    CheckoutAction::Delete
                },

            // case 3/4/6
            // TODO case 6 (if ignored)
            TreeDiffEntry::CreatedBlob(..) => cond!(self.opts.is_force() => Update : Conflict),
            // case 16/17/18: B1 B2 (B1|B2|B3)
            TreeDiffEntry::ModifiedBlob(..) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 17/18: B1 B2 (B2|B3)
                    cond!(self.opts.is_force() => Update : Conflict)
                } else {
                    // case 16: B1 B2 B1 | update unmodified blob
                    CheckoutAction::Update
                },
            // case 14/15: B1 B1 B1/B2
            TreeDiffEntry::UnmodifiedBlob(..) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 15: B1 B1 B2 | locally modified file (dirty)
                    // change is only applied to index if forced
                    cond!(self.opts.is_force() => Update : None)
                } else {
                    CheckoutAction::None
                },
            TreeDiffEntry::DeletedTree(..) => todo!(),
            TreeDiffEntry::CreatedTree(..) => todo!(),
        };
        debug_assert_eq!(diff_entry.path_mode(), worktree_entry.path_mode());
        Ok((worktree_entry.into(), action))
    }

    fn action_no_worktree(
        &mut self,
        diff_entry: TreeDiffEntry<'_>,
    ) -> BitResult<(TreeEntry, CheckoutAction)> {
        let action = match diff_entry {
            // case 8: B1 x x | delete blob (safe + missing)
            TreeDiffEntry::DeletedBlob(..) => CheckoutAction::Delete,
            // case 2: x B1 x | create blob (safe)
            TreeDiffEntry::CreatedBlob(..) => CheckoutAction::Create,
            // case 13: B1 B2 x | modify/delete conflict
            TreeDiffEntry::ModifiedBlob(..) => CheckoutAction::Conflict,
            // case 12: B1 B1 x | locally deleted blob (safe + missing)
            TreeDiffEntry::UnmodifiedBlob(..) => CheckoutAction::Delete,
            TreeDiffEntry::DeletedTree(..) => todo!(),
            TreeDiffEntry::CreatedTree(..) => todo!(),
        };
        Ok((diff_entry.tree_entry(), action))
    }
}

#[derive(Default, Debug)]
struct MigrationDiffer {
    migration: Migration,
}

impl TreeDiffBuilder for MigrationDiffer {
    type Output = Migration;

    fn get_output(self) -> Self::Output {
        self.migration
    }
}

impl TreeDiffer for MigrationDiffer {
    fn created_tree(&mut self, entries_consumer: TreeEntriesConsumer<'_>) -> BitResult<()> {
        let mut entries = vec![];
        let tree_entry = entries_consumer.collect_over_all(&mut entries)?;
        debug_assert_ne!(
            tree_entry.path,
            BitPath::EMPTY,
            "should not be creating a root directory, probably occurred to an iterator not yielding a root"
        );
        self.migration.mkdirs.push(tree_entry.into());

        for entry in entries {
            match entry.mode {
                FileMode::REG | FileMode::EXEC | FileMode::LINK =>
                    self.migration.creates.push(entry.into()),
                FileMode::TREE => self.migration.mkdirs.push(entry.into()),
                FileMode::GITLINK => todo!(),
            }
        }
        Ok(())
    }

    fn created_blob(&mut self, new: BitIndexEntry) -> BitResult<()> {
        Ok(self.migration.creates.push(new.into()))
    }

    fn deleted_tree(&mut self, entries: TreeEntriesConsumer<'_>) -> BitResult<()> {
        let entry = entries.step_over()?;
        Ok(self.migration.rmrfs.push(entry.into()))
    }

    fn deleted_blob(&mut self, old: BitIndexEntry) -> BitResult<()> {
        Ok(self.migration.rms.push(old.into()))
    }

    fn modified_blob(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        // we could generate a patch and use that, but is that really faster than just removing the old and recreating the new?
        self.deleted_blob(old)?;
        self.created_blob(new)
    }
}

#[cfg(test)]
mod migration_gen_tests;
#[cfg(test)]
mod tests;
