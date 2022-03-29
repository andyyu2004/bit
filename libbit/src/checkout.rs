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

#[derive(Debug)]
pub struct CheckoutSummary {
    pub new_head: BitRef,
}

impl BitRepo {
    /// checkout the branch/commit specified by the revision
    /// - updates the worktree to match the tree represented by the tree of the commit
    /// - moves HEAD to point at the branch/commit
    pub fn checkout_revision(
        &self,
        rev: &Revspec,
        opts: CheckoutOpts,
    ) -> BitResult<CheckoutSummary> {
        let reference = self.resolve_rev(rev)?;
        self.checkout(reference, opts)
    }

    pub fn checkout(
        &self,
        reference: impl Into<BitRef>,
        opts: CheckoutOpts,
    ) -> BitResult<CheckoutSummary> {
        let reference: BitRef = reference.into();
        // doesn't make sense to move HEAD -> HEAD
        assert_ne!(reference, BitRef::HEAD);

        let commit_oid = self.fully_resolve_ref(reference)?;
        self.checkout_tree_with_opts(commit_oid, opts)?;

        // checking out a remote reference should puts us in detached head state
        // as we should not be allowed to modify remote branches locally
        let new_head = if reference.is_remote() { commit_oid.into() } else { reference };
        self.update_head_for_checkout(new_head)?;

        Ok(CheckoutSummary { new_head })
    }

    pub fn checkout_tree(&self, treeish: impl Treeish) -> BitResult<()> {
        self.checkout_tree_with_opts(treeish, CheckoutOpts::default())
    }

    pub fn force_checkout_tree(&self, treeish: impl Treeish) -> BitResult<()> {
        self.checkout_tree_with_opts(treeish, CheckoutOpts::forced())
    }

    /// Update working directory and index to match the tree referenced by `treeish`, accounting for changes in the working tree.
    // IMPORTANT
    // - Don't update HEAD before calling this as this does a diff relative to the current HEAD (for now)
    pub fn checkout_tree_with_opts(
        &self,
        treeish: impl Treeish,
        opts: CheckoutOpts,
    ) -> BitResult<()> {
        let target_tree = treeish.treeish_oid(self)?;
        let target = self.tree_iter(target_tree);

        let is_forced = opts.is_forced();
        self.checkout_iterator(target, opts)?;

        // if forced, then the worktree and index and target_tree should match exactly
        if is_forced {
            let mut index = self.index_mut()?;
            debug_assert!(index.diff_tree(target_tree, Pathspec::MATCH_ALL)?.is_empty());
            debug_assert!(index.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
            index.update_cache_tree(target_tree)?;
        }
        Ok(())
    }

    pub fn checkout_index(&self, opts: CheckoutOpts) -> BitResult<()> {
        // TODO
        // We need to clone the index to obtain an iterator avoid "concurrent" reads and writes
        // as checkout uses the index to determine whether files have been modified etc
        // Indexes are usually not too massive so isn't the end of the world.
        // But the design of the index is causing pain points all round so
        let cloned = self.index()?.clone();
        self.checkout_iterator(cloned.index_tree_iter(), opts)
    }

    pub fn checkout_iterator(
        &self,
        target: impl BitTreeIterator,
        opts: CheckoutOpts,
    ) -> BitResult<()> {
        let baseline = self.head_tree_iter()?;
        let worktree = self.index()?.worktree_tree_iter()?;
        let migration = self.generate_migration(baseline, target, worktree, opts)?;
        self.apply_migration(&migration)
    }

    fn generate_migration(
        &self,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: impl BitTreeIterator,
        opts: CheckoutOpts,
    ) -> BitResult<Migration> {
        CheckoutCtxt::new(self.clone(), opts).generate(baseline, target, worktree)
    }

    fn apply_migration(&self, migration: &Migration) -> BitResult<()> {
        let mut index = self.index_mut()?;
        migration.rmrfs.iter().try_for_each(|rmrf| {
            let path = self.to_absolute_path(&rmrf.path);
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            }
            index.remove_directory(rmrf.path)
        })?;

        for rm in &migration.rms {
            let path = self.to_absolute_path(&rm.path);
            if path.is_file() {
                std::fs::remove_file(&path)
                    .with_context(|| anyhow!("failed to remove file in `apply_migration`"))?;

                let parent = path.parent().expect("a file must have a parent");
                // if we remove a file and that results in the directory being empty
                // we can just remove the directory too
                if parent.read_dir()?.next().is_none() {
                    std::fs::remove_dir(parent)?;
                }
            }
            index.remove_entry((rm.path, MergeStage::None));
        }

        for mkdir in &migration.mkdirs {
            let path = self.to_absolute_path(&mkdir.path);
            debug_assert!(!path.is_file());
            // TODO think there's a bug somewhere that requires this to be `create_dir_all`
            // Go to any sufficiently large repo and just do something like `bit checkout @~500~
            // and there's likely to be a directory that failed to be created
            // due to the parent directories not existing
            std::fs::create_dir_all(&path).with_context(|| {
                anyhow!("failed to create directory `{}`, in `apply_migration`", mkdir.path)
            })?;
        }

        for create in &migration.creates {
            let path = self.to_absolute_path(&create.path);
            let bytes = create.read_to_bytes(self)?;
            // this is necessary due to `rm` above deleting empty directories that may be repopulated
            // there is probably a better way
            std::fs::create_dir_all(path.parent().unwrap())?;

            if create.mode.is_link() {
                //? is it guaranteed that a symlink contains the path of the target, or is it fs impl dependent?
                let symlink_target = OsStr::from_bytes(&bytes);
                std::os::unix::fs::symlink(symlink_target, path)?;
            } else {
                debug_assert!(create.mode.is_file());
                let mut file = std::fs::File::options()
                    .create_new(true)
                    .read(false)
                    .write(true)
                    .open(&path)
                    .with_context(|| {
                        anyhow!("failed to create file `{}` in `apply_migration`", path)
                    })?;
                file.write_all(&bytes)?;
                file.set_permissions(Permissions::from_mode(create.mode.as_u32()))?;
            }

            let mut index_entry = BitIndexEntry::from_absolute_path(self, &path)?;
            index_entry.oid = create.oid;
            index.add_entry(index_entry)?;
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
        repo: BitRepo,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: impl BitTreeIterator,
        opts: CheckoutOpts,
    ) -> BitResult<Self> {
        CheckoutCtxt::new(repo, opts).generate(baseline, target, worktree)
    }
}

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CheckoutConflicts {
    worktree: Vec<TreeEntry>,
}

impl CheckoutConflicts {
    pub fn len(&self) -> usize {
        self.worktree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.worktree.is_empty()
    }
}

pub struct CheckoutCtxt {
    repo: BitRepo,
    migration: Migration,
    opts: CheckoutOpts,
    conflicts: CheckoutConflicts,
}

// yep, really writing a macro for an if expression?
// helps keep the condition on one line
macro_rules! cond {
    ($pred:expr => $then:expr; $otherwise:expr) => {
        if $pred { $then? } else { $otherwise? }
    };
    ($pred:expr => $then:expr) => {
        if $pred {
            $then?
        }
    };
}

impl CheckoutCtxt {
    pub fn new(repo: BitRepo, opts: CheckoutOpts) -> Self {
        Self { repo, opts, migration: Default::default(), conflicts: Default::default() }
    }

    // Refer to https://github.com/libgit2/libgit2/blob/main/docs/checkout-internals.md
    // Following source code more closely than the table though
    pub fn generate(
        mut self,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        mut worktree: impl BitTreeIterator,
    ) -> BitResult<Migration> {
        self.checkout(baseline, target, &mut worktree)?;

        // consume the remaining unmatched worktree entries
        while let Some(worktree_entry) = worktree.peek()? {
            self.worktree_only(&mut worktree, worktree_entry)?
        }

        if self.conflicts.is_empty() {
            Ok(self.migration)
        } else {
            bail!(BitError::CheckoutConflict(self.conflicts))
        }
    }

    fn checkout(
        &mut self,
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
        worktree: &mut impl BitTreeIterator,
    ) -> BitResult<()> {
        todo!()
        // let diff_iter =
        //     self.repo.tree_diff_iter_with_opts(baseline, target, DiffOpts::INCLUDE_UNMODIFIED);

        // diff_iter.into_iter().for_each(|diff_entry| {
        //     loop {
        //         let worktree_entry = worktree.peek()?;
        //         debug!(
        //             "CheckoutCtxt::generate({:#?}, {:#?})",
        //             diff_entry,
        //             worktree_entry.map(|entry| TreeEntry::from(entry))
        //         );
        //         // matchup the worktree iterator with the diff iterator by comparing order of entries
        //         match worktree_entry {
        //             Some(worktree_entry) => {
        //                 // to avoid the unfortunate `ModifiedTree` case we can probably do
        //                 // something like `if diff_entry.path().starts_with(worktree_entry.path()) { skip this worktree_entry as it's a tree entry that has been modified }`
        //                 // to keep them in sync
        //                 match worktree_entry.diff_cmp(&diff_entry.index_entry()) {
        //                     // worktree behind diffs, process worktree_entry alone
        //                     Ordering::Less => self.worktree_only(worktree, worktree_entry)?,
        //                     // worktree even with diffs, process diff_entry and worktree_entry together
        //                     Ordering::Equal =>
        //                         break self.with_worktree(worktree, diff_entry, worktree_entry),
        //                     // worktree ahead of diffs, process only diff_entry
        //                     Ordering::Greater => break self.no_worktree(diff_entry),
        //                 }
        //             }
        //             // again, worktree ahead of diffs
        //             None => break self.no_worktree(diff_entry),
        //         }
        //     }
        // })
    }

    fn worktree_only(
        &mut self,
        worktree: &mut impl BitTreeIterator,
        worktree_entry: BitIndexEntry,
    ) -> BitResult<()> {
        // TODO consider .gitignore rules
        match worktree_entry.mode() {
            FileMode::REG | FileMode::EXEC | FileMode::LINK => {
                worktree.next()?;
                cond!(self.opts.is_forced() => self.delete(worktree_entry));
            }
            FileMode::TREE =>
                if self.opts.is_forced() {
                    self.delete_tree(worktree.as_consumer())?
                } else {
                    worktree.over()?;
                },
            FileMode::GITLINK => todo!(),
        };
        Ok(())
    }

    fn with_worktree_dir(
        &mut self,
        worktree: &mut impl BitTreeIterator,
        diff_entry: TreeDiffEntry<'_>,
        worktree_entry: BitIndexEntry,
    ) -> BitResult<()> {
        match diff_entry {
            // case 11: B1 x T1 | independently deleted blob and untracked/ignored tree
            TreeDiffEntry::DeletedBlob(blob) => {
                self.delete(blob)?;
                cond!(self.opts.is_forced() => self.delete_tree(worktree.as_consumer()))
            }
            TreeDiffEntry::CreatedBlob(_) => todo!(),
            // case 20:  B1 B2 T1
            TreeDiffEntry::ModifiedBlob(_, entry) => {
                let consumer = worktree.as_consumer();
                cond!(self.opts.is_forced() => self.tree_to_blob(consumer, entry); self.conflict(consumer.step_over()?))
            }
            // case 37: T1 T2 T1/T2/T3 | update to existing tree
            TreeDiffEntry::MaybeModifiedTree(_) => {
                worktree.next()?;
            }
            // case 19: B1 B1 T1?
            TreeDiffEntry::UnmodifiedBlob(_) => todo!(),
            // case 34: T1 T1 T1/T2 | unmodified tree
            // Be wary that we could be currently be in the root directory so don't do a rm -rf here..
            TreeDiffEntry::UnmodifiedTree(tree) =>
                if self.opts.is_forced() {
                    self.reset_worktree(worktree, tree)?;
                } else {
                    // otherwise we can just keep all changes from the working tree
                    worktree.over()?;
                },
            // case 27: T1 x T1/T2 (maybe safe)
            TreeDiffEntry::DeletedTree(tree) => {
                if self.opts.is_forced() {
                    return self.delete_worktree_tree(worktree_entry);
                }
                // It is safe to remove if there are no local changes
                let has_local_changes =
                    self.repo.trees_are_diff(tree.iter(), worktree.as_consumer().iter())?;
                if has_local_changes {
                    self.conflict(worktree_entry)?
                } else {
                    self.delete_worktree_tree(worktree_entry)?
                }
            }
            // case 7: x T1 T1/T2 | independently added tree
            TreeDiffEntry::CreatedTree(tree) =>
                if self.opts.is_forced() {
                    self.reset_created_worktree(worktree, tree)?
                } else {
                    worktree.over()?;
                    self.conflict(tree.step_over()?)?
                },
            // case 24: B1 T1 T1/T2 | add tree with deleted blob (forceable)
            // TODO implementation is exactly the same as above case?
            TreeDiffEntry::BlobToTree(_, tree) =>
                if self.opts.is_forced() {
                    self.reset_created_worktree(worktree, tree)?
                } else {
                    worktree.over()?;
                    self.conflict(tree.step_over()?)?
                },
            // case 31 T1 B1 T1/T2 | typechange tree->blob
            TreeDiffEntry::TreeToBlob(tree, blob) =>
                if self.opts.is_forced() {
                    self.tree_to_blob(tree, blob)?
                } else {
                    // It is safe to replace tree with blob if there are no local changes
                    let has_local_changes =
                        self.repo.trees_are_diff(tree.iter(), worktree.as_consumer().iter())?;
                    if has_local_changes {
                        self.conflict(blob)?
                    } else {
                        self.delete_worktree_tree(worktree_entry)?;
                        self.create(blob)?;
                    }
                },
        }
        Ok(())
    }

    fn with_worktree(
        &mut self,
        worktree: &mut impl BitTreeIterator,
        diff_entry: TreeDiffEntry<'_>,
        mut worktree_entry: BitIndexEntry,
    ) -> BitResult<()> {
        if worktree_entry.is_tree() {
            self.with_worktree_dir(worktree, diff_entry, worktree_entry)?
        } else {
            worktree.next()?;
            let mut index = self.repo.index_mut()?;
            match diff_entry {
                // case 9/case 10: B1 x B1|B2
                TreeDiffEntry::DeletedBlob(entry) =>
                    if index.is_worktree_entry_modified(&mut worktree_entry)? {
                        // case 10: B1 x B2 | delete of modified blob (forceable)
                        cond!(self.opts.is_forced() => self.delete(worktree_entry); self.conflict(entry))
                    } else {
                        // case 9: B1 x B1 | delete blob (safe)
                        self.delete(entry)?
                    },

                // case 3/4/6
                // TODO case 6 (if ignored)
                TreeDiffEntry::CreatedBlob(entry) =>
                    cond!(self.opts.is_forced() => self.update(entry); self.conflict(entry)),
                // case 16/17/18: B1 B2 (B1|B2|B3)
                TreeDiffEntry::ModifiedBlob(_, entry) =>
                    if index.is_worktree_entry_modified(&mut worktree_entry)? {
                        // case 17/case 18: B1 B2 (B2|B3)
                        cond!(self.opts.is_forced() => self.update(entry); self.conflict(entry))
                    } else {
                        // case 16: B1 B2 B1 | update unmodified blob
                        self.update(entry)?
                    },
                // case 36: T1 T2 B1/Bi | update to tree with typechanged tree->blob conflict (forceable)
                TreeDiffEntry::MaybeModifiedTree(..) =>
                    if !self.opts.is_forced() {
                        // TODO this could be smarter as this is only "maybe modified" but we're assuming it is modified (i.e. case 36)
                        // If it's not forced, then this should be a conflict.
                        self.conflict(worktree_entry)?;
                    } else {
                        // Otherwise, we can just step in and continue after deleting the worktree blob
                        self.delete(worktree_entry)?;
                    },
                // case 14/case 15: B1 B1 B1/B2
                TreeDiffEntry::UnmodifiedBlob(entry) =>
                    if index.is_worktree_entry_modified(&mut worktree_entry)? {
                        // case 15: B1 B1 B2 | locally modified file (dirty)
                        // change is only applied to index if forced
                        cond!(self.opts.is_forced() => self.update(entry))
                    } else {
                        // case 14: B1 B1 B1 | unmodified file
                    },
                TreeDiffEntry::UnmodifiedTree(_) => todo!(),
                // case 26
                TreeDiffEntry::DeletedTree(tree) =>
                    if self.opts.is_forced() {
                        // delete the actual worktree entry on disk
                        self.delete(worktree_entry)?;
                        // then delete the index records of the tree
                        self.delete_tree(tree)?
                    } else {
                        self.conflict(worktree_entry)?
                    },
                // case 6: x T1 B1/Bi | add tree with blob conflict (forceable)
                // TODO ignored case
                TreeDiffEntry::CreatedTree(tree) =>
                    if self.opts.is_forced() {
                        // replace worktree blob with target tree
                        self.blob_to_tree(worktree_entry, tree)?
                    } else {
                        self.conflict(tree.step_over()?)?
                    },
                // case 22/case 23: B1 T1 B1/B2
                TreeDiffEntry::BlobToTree(blob, tree) =>
                    if index.is_worktree_entry_modified(&mut worktree_entry)? {
                        // case 22
                        cond!(self.opts.is_forced() => self.blob_to_tree(blob, tree); self.conflict(worktree_entry))
                    } else {
                        // case 23
                        self.blob_to_tree(blob, tree)?
                    },
                // case 29/case 30: T1 B1 B1|B2 | (forceable)
                TreeDiffEntry::TreeToBlob(tree, blob) =>
                    if self.opts.is_forced() {
                        self.delete(worktree_entry)?;
                        self.tree_to_blob(tree, blob)?
                    } else {
                        self.conflict(worktree_entry)?
                    },
            }
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
            TreeDiffEntry::ModifiedBlob(_, entry) =>
                cond!(self.opts.is_forced() => self.update(entry); self.conflict(entry)),
            TreeDiffEntry::MaybeModifiedTree(..) => {}
            // case 12: B1 B1 x | locally deleted blob (safe + missing)
            TreeDiffEntry::UnmodifiedBlob(blob) =>
                cond!(self.opts.is_forced() => self.create(blob)),
            // case 32: T1 T1 x
            TreeDiffEntry::UnmodifiedTree(tree) =>
                cond!(self.opts.is_forced() => self.create_tree(tree)),
            // case 25: T1 x x | independently deleted tree (safe + missing)
            TreeDiffEntry::DeletedTree(tree) =>
                cond!(self.opts.is_safe() => self.delete_tree(tree)),
            // case 5: x T1 x | created tree
            TreeDiffEntry::CreatedTree(entries) => self.create_tree(entries)?,
            TreeDiffEntry::BlobToTree(blob, tree) => self.blob_to_tree(blob, tree)?,
            TreeDiffEntry::TreeToBlob(tree, blob) => self.tree_to_blob(tree, blob)?,
        };
        Ok(())
    }

    /// TODO naming
    fn reset_created_worktree(
        &mut self,
        worktree: &mut impl BitTreeIterator,
        tree: TreeEntriesConsumer<'_>,
    ) -> BitResult<()> {
        let mut tree_iter = tree.iter();
        debug_assert_eq!(worktree.peek()?.unwrap().path(), tree_iter.peek()?.unwrap().path());
        // we have to consume the `root` entry and create the directory otherwise the recursive
        // call to `checkout` may just recurse indefinitely
        self.migration.mkdirs.push(tree_iter.next()?.unwrap().into());
        self.checkout(self.repo.empty_tree_iter(), tree_iter, worktree)
    }

    /// Reset the the current worktree tree to exactly match the subtree `tree`
    /// Assumes that the baseline and target tree are exactly matching?
    fn reset_worktree(
        &mut self,
        worktree: &mut impl BitTreeIterator,
        tree: TreeEntriesConsumer<'_>,
    ) -> BitResult<()> {
        todo!()
        // The cases are all "backwards" as we always want `worktree` to be the second argument to tree_diffs
        // self.repo
        //     .tree_diff_iter_with_opts(tree.iter(), worktree, DiffOpts::INCLUDE_UNMODIFIED)
        //     .for_each(|diff_entry: TreeDiffEntry<'_>| match diff_entry {
        //         TreeDiffEntry::DeletedBlob(blob) => self.create(blob),
        //         TreeDiffEntry::CreatedBlob(blob) => self.delete(blob),
        //         TreeDiffEntry::ModifiedBlob(entry, wt) => {
        //             self.delete(wt)?;
        //             self.create(entry)
        //         }
        //         TreeDiffEntry::UnmodifiedBlob(..) => Ok(()),
        //         TreeDiffEntry::MaybeModifiedTree(..) => Ok(()),
        //         TreeDiffEntry::UnmodifiedTree(..) => unreachable!(),
        //         TreeDiffEntry::DeletedTree(tree) => self.create_tree(tree),
        //         TreeDiffEntry::CreatedTree(tree) => self.delete_tree(tree),
        //         TreeDiffEntry::BlobToTree(blob, tree) => self.tree_to_blob(tree, blob),
        //         TreeDiffEntry::TreeToBlob(tree, blob) => self.blob_to_tree(blob, tree),
        //     })
    }

    fn update(&mut self, entry: impl Into<TreeEntry>) -> BitResult<()> {
        let entry = entry.into();
        match entry.mode {
            FileMode::REG | FileMode::EXEC | FileMode::LINK => self.migration.rms.push(entry),
            FileMode::TREE => self.migration.rmrfs.push(entry),
            FileMode::GITLINK => todo!(),
        }
        self.migration.creates.push(entry);
        Ok(())
    }

    fn conflict(&mut self, entry: impl Into<TreeEntry>) -> BitResult<()> {
        self.conflicts.worktree.push(entry.into());
        Ok(())
    }

    fn create(&mut self, entry: impl Into<TreeEntry>) -> BitResult<()> {
        self.migration.creates.push(entry.into());
        Ok(())
    }

    fn mkdir(&mut self, entry: impl Into<TreeEntry>) -> BitResult<()> {
        self.migration.mkdirs.push(entry.into());
        Ok(())
    }

    // First, create the root of the subtree
    // then take all entries within the subtree and create them appropriately
    // `entries` currently includes the root of the tree
    fn create_tree(&mut self, tree: TreeEntriesConsumer<'_>) -> BitResult<()> {
        tree.iter().for_each(|entry: BitIndexEntry| {
            match entry.mode() {
                FileMode::REG | FileMode::EXEC | FileMode::LINK => self.create(entry),
                FileMode::TREE => Ok(if entry.path() != BitPath::EMPTY {
                    self.mkdir(entry)?
                }),
                FileMode::GITLINK => todo!(),
            }?;
            Ok(())
        })
    }

    fn tree_to_blob(
        &mut self,
        tree: TreeEntriesConsumer<'_>,
        blob: impl Into<TreeEntry>,
    ) -> BitResult<()> {
        self.delete_tree(tree)?;
        self.create(blob)
    }

    fn blob_to_tree(
        &mut self,
        blob: impl Into<TreeEntry>,
        tree: TreeEntriesConsumer<'_>,
    ) -> BitResult<()> {
        self.delete(blob)?;
        self.create_tree(tree)
    }

    fn delete_worktree_tree(&mut self, tree: impl Into<TreeEntry>) -> BitResult<()> {
        self.migration.rmrfs.push(tree.into());
        Ok(())
    }

    fn delete_tree(&mut self, tree: TreeEntriesConsumer<'_>) -> BitResult<()> {
        self.migration.rmrfs.push(tree.step_over()?.into());
        Ok(())
    }

    fn delete(&mut self, entry: impl Into<TreeEntry>) -> BitResult<()> {
        self.migration.rms.push(entry.into());
        Ok(())
    }
}

#[cfg(test)]
mod migration_gen_tests;
#[cfg(test)]
mod tests;
