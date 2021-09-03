use crate::diff::*;
use crate::error::{BitError, BitResult};
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

    fn is_forced(&self) -> bool {
        self.strategy >= CheckoutStrategy::Force
    }

    fn is_safe(&self) -> bool {
        self.strategy >= CheckoutStrategy::Safe
    }
}

// Each strategy level implies the level above
// i.e. Force => Safe
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckoutStrategy {
    None,
    Safe,
    // Forced checkout will result in the index and working tree exactly matching the target tree
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

        let commit_oid = self.fully_resolve_ref(reference)?;
        self.checkout_tree_with_opts(commit_oid, opts)?;

        // checking out a remote reference should puts us in detached head state
        // as we should not be allowed to modify remote branches locally
        let new_head = if reference.is_remote() { commit_oid.into() } else { reference };
        self.update_head_for_checkout(new_head)?;

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

    pub fn force_checkout_tree(self, treeish: impl Treeish<'rcx>) -> BitResult<()> {
        self.checkout_tree_with_opts(treeish, CheckoutOpts::forced())
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
        #[cfg(debug_assertions)]
        let is_forced = opts.is_forced();
        let baseline = repo.head_tree_iter()?;
        let target = repo.tree_iter(target_tree);
        let worktree = self.worktree_iter()?;
        let migration = self.generate_migration(baseline, target, worktree, opts)?;
        self.apply_migration(&migration)?;

        // if forced, then the worktree and index should match exactly
        if is_forced {
            debug_assert!(self.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
            debug_assert!(self.diff_tree(target_tree, Pathspec::MATCH_ALL)?.is_empty());
        }
        Ok(())
    }

    pub fn force_checkout_tree(&mut self, treeish: impl Treeish<'rcx>) -> BitResult<()> {
        self.checkout_tree_with_opts(treeish, CheckoutOpts::forced())
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
            let path = repo.to_absolute_path(&rm.path);
            std::fs::remove_file(path)?;
            // if we remove a file and that results in the directory being empty
            // we can just remove the directory too
            let parent = path.parent().expect("a file must have a parent");
            if parent.read_dir()?.next().is_none() {
                std::fs::remove_dir(parent)?;
            }
            self.remove_entry((rm.path, MergeStage::None));
        }

        for mkdir in &migration.mkdirs {
            std::fs::create_dir(repo.to_absolute_path(&mkdir.path))
                .with_context(|| anyhow!("failed to create directory in `apply_migration`"))?;
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
                    .with_context(|| anyhow!("failed to create file in `apply_migration`"))?;
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

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CheckoutConflicts {
    worktree: Vec<TreeEntry>,
}

impl CheckoutConflicts {
    pub fn is_empty(&self) -> bool {
        self.worktree.is_empty()
    }
}

pub struct CheckoutCtxt<'a, 'rcx> {
    repo: BitRepo<'rcx>,
    index: &'a mut BitIndex<'rcx>,
    migration: Migration,
    opts: CheckoutOpts,
    conflicts: CheckoutConflicts,
}

// yep, really writing a macro for an if expression?
// helps keep the condition on one line
macro_rules! cond {
    ($pred:expr => $then:expr; $otherwise:expr) => {
        if $pred { $then } else { $otherwise }
    };
    ($pred:expr => $then:expr) => {
        if $pred {
            $then
        }
    };
}

impl<'a, 'rcx> CheckoutCtxt<'a, 'rcx> {
    pub fn new(index: &'a mut BitIndex<'rcx>, opts: CheckoutOpts) -> Self {
        let repo = index.repo;
        Self { index, repo, opts, migration: Default::default(), conflicts: Default::default() }
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

        diff_iter.for_each(|diff_entry| {
            // matchup the worktree iterator with the diff iterator by comparing order of entries
            match worktree_iter.peek()? {
                Some(worktree_entry) => match worktree_entry.entry_cmp(&diff_entry.index_entry()) {
                    // worktree behind diffs, process worktree_entry alone
                    Ordering::Less => {
                        self.worktree_only(worktree_entry)?;
                        worktree_iter.next()?;
                        Ok(())
                    }
                    // worktree even with diffs, process diff_entry and worktree_entry together
                    Ordering::Equal => {
                        self.with_worktree(diff_entry, worktree_entry)?;
                        worktree_iter.next()?;
                        Ok(())
                    }
                    // worktree ahead of diffs, process only diff_entry
                    Ordering::Greater => self.no_worktree(diff_entry),
                },
                // again, worktree ahead of diffs
                None => self.no_worktree(diff_entry),
            }
        })?;

        // consume the remaining unmatched worktree entries
        worktree_iter.for_each(|worktree_entry| self.worktree_only(&worktree_entry))?;

        if self.conflicts.is_empty() {
            Ok(self.migration)
        } else {
            bail!(BitError::CheckoutConflict(self.conflicts))
        }
    }

    fn worktree_only(&mut self, worktree_entry: &BitIndexEntry) -> BitResult<()> {
        // TODO consider .gitignore rules
        cond!(self.opts.is_forced() => self.delete(worktree_entry));
        Ok(())
    }

    fn with_worktree(
        &mut self,
        diff_entry: TreeDiffEntry<'_>,
        worktree_entry: &BitIndexEntry,
    ) -> BitResult<()> {
        match diff_entry {
            // case 9/10: B1 x B1|B2
            TreeDiffEntry::DeletedBlob(entry) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 10: B1 x B2 | delete of modified blob (forceable)
                    cond!(self.opts.is_forced() => self.delete(entry); self.conflict(entry))
                } else {
                    // case 9: B1 x B1 | delete blob (safe)
                    self.delete(entry);
                },

            // case 3/4/6
            // TODO case 6 (if ignored)
            TreeDiffEntry::CreatedBlob(entry) =>
                cond!(self.opts.is_forced() => self.update(entry); self.conflict(entry)),
            // case 16/17/18: B1 B2 (B1|B2|B3)
            TreeDiffEntry::ModifiedBlob(_, entry) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 17/18: B1 B2 (B2|B3)
                    cond!(self.opts.is_forced() => self.update(entry); self.conflict(entry))
                } else {
                    // case 16: B1 B2 B1 | update unmodified blob
                    self.update(entry);
                },
            // case 14/15: B1 B1 B1/B2
            TreeDiffEntry::UnmodifiedBlob(entry) =>
                if self.index.is_worktree_entry_modified(worktree_entry)? {
                    // case 15: B1 B1 B2 | locally modified file (dirty)
                    // change is only applied to index if forced
                    cond!(self.opts.is_forced() => self.update(entry))
                } else {
                    // case 14: B1 B1 B1 | unmodified file
                    ()
                },
            TreeDiffEntry::DeletedTree(..) => todo!(),
            TreeDiffEntry::CreatedTree(..) => todo!(),
            TreeDiffEntry::BlobToTree(_, _) => todo!(),
            TreeDiffEntry::TreeToBlob(_, _) => todo!(),
        };
        Ok(())
    }

    fn no_worktree(&mut self, diff_entry: TreeDiffEntry<'_>) -> BitResult<()> {
        match diff_entry {
            // case 8: B1 x x | delete blob (safe + missing)
            // TODO our current implementation of delete won't work
            // as during safe checkout we will try to delete a file that doesn't even exist
            TreeDiffEntry::DeletedBlob(entry) => cond!(self.opts.is_safe() => self.delete(entry)),
            // case 2: x B1 x | create blob (safe)
            TreeDiffEntry::CreatedBlob(entry) => cond!(self.opts.is_safe() => self.create(entry)),
            // case 13: B1 B2 x | modify/delete conflict
            // TODO what if forced?
            TreeDiffEntry::ModifiedBlob(_, entry) => self.conflict(entry),
            // case 12: B1 B1 x | locally deleted blob (safe + missing)
            // TODO what if forced?
            TreeDiffEntry::UnmodifiedBlob(blob) => self.delete(blob),
            // case 25: T1 x x | independently deleted tree (safe + missing)
            TreeDiffEntry::DeletedTree(tree) =>
                cond!(self.opts.is_safe() => self.delete_tree(tree)?),
            TreeDiffEntry::CreatedTree(entries) => self.create_tree(entries)?,
            TreeDiffEntry::BlobToTree(blob, tree) => {
                self.delete(blob);
                self.create_tree(tree)?;
            }
            TreeDiffEntry::TreeToBlob(tree, blob) => {
                self.delete_tree(tree)?;
                self.create(blob);
            }
        };
        Ok(())
    }

    fn update(&mut self, entry: impl Into<TreeEntry>) {
        let entry = entry.into();
        match entry.mode {
            FileMode::REG | FileMode::EXEC | FileMode::LINK => self.migration.rms.push(entry),
            FileMode::TREE => self.migration.rmrfs.push(entry),
            FileMode::GITLINK => todo!(),
        }
        self.migration.creates.push(entry);
    }

    fn conflict(&mut self, entry: impl Into<TreeEntry>) {
        self.conflicts.worktree.push(entry.into());
    }

    fn create(&mut self, entry: impl Into<TreeEntry>) {
        self.migration.creates.push(entry.into());
    }

    fn mkdir(&mut self, entry: impl Into<TreeEntry>) {
        self.migration.mkdirs.push(entry.into());
    }

    // First, create the root of the subtree
    // then take all entries within the subtree and create them appropriately
    // `entries` currently includes the root of the tree
    fn create_tree(&mut self, tree: TreeEntriesConsumer<'_>) -> BitResult<()> {
        tree.iter().for_each(|entry: BitIndexEntry| {
            match entry.mode() {
                FileMode::REG | FileMode::EXEC | FileMode::LINK => self.create(entry),
                FileMode::TREE => self.mkdir(entry),
                FileMode::GITLINK => todo!(),
            };
            Ok(())
        })
    }

    fn delete_tree(&mut self, tree: TreeEntriesConsumer<'_>) -> BitResult<()> {
        self.migration.rmrfs.push(tree.step_over()?.into());
        Ok(())
    }

    fn delete(&mut self, entry: impl Into<TreeEntry>) {
        self.migration.rms.push(entry.into())
    }
}

#[cfg(test)]
mod migration_gen_tests;
#[cfg(test)]
mod tests;
