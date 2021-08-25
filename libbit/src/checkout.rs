use crate::diff::{TreeDiffBuilder, TreeDiffer, TreeEntriesConsumer};
use crate::error::BitResult;
use crate::index::{BitIndex, BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitTreeIterator};
use crate::obj::{FileMode, TreeEntry, Treeish};
use crate::pathspec::Pathspec;
use crate::refs::{BitRef, RefUpdateCause};
use crate::repo::BitRepo;
use crate::rev::Revspec;
use std::ffi::OsStr;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::prelude::{OsStrExt, PermissionsExt};

impl<'rcx> BitRepo<'rcx> {
    /// checkout the branch/commit specified by the revision
    /// - updates the worktree to match the tree represented by the tree of the commit
    /// - moves HEAD to point at the branch/commit
    pub fn checkout(self, rev: &Revspec) -> BitResult<()> {
        let reference = self.resolve_rev(rev)?;
        self.checkout_reference(reference)
    }

    pub fn checkout_reference(self, reference: impl Into<BitRef>) -> BitResult<()> {
        let reference: BitRef = reference.into();
        // doesn't make sense to move HEAD -> HEAD
        assert_ne!(reference, BitRef::HEAD);

        let status = self.status(Pathspec::MATCH_ALL)?;
        // only allow checkout on fully clean states for now
        if !status.is_empty() {
            bail!("cannot checkout: unclean state")
        }

        let commit_oid = self.fully_resolve_ref(reference)?;
        self.checkout_tree(commit_oid)?;

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

    fn update_head_for_checkout(self, to: impl Into<BitRef>) -> BitResult<()> {
        let to = to.into();
        self.update_head(to, RefUpdateCause::Checkout { from: self.read_head()?, to })
    }

    /// Update working directory and index to match the tree referenced by `treeish`
    // there are currently no safety checks here! (i.e. it does a force checkout)
    pub fn checkout_tree(self, treeish: impl Treeish<'rcx>) -> BitResult<()> {
        let target_tree = treeish.treeish_oid(self)?;
        let baseline = self.head_tree_iter()?;
        let target = self.tree_iter(target_tree);
        // TODO take current workdir into account (and weaken the assertions in checkout that require a clean status)
        // let workdir = self.worktree_iter()?;
        let migration = Migration::generate(baseline, target)?;
        self.with_index_mut(|index| {
            index.apply_migration(&migration)?;
            debug_assert!(index.diff_worktree(Pathspec::MATCH_ALL)?.is_empty());
            debug_assert!(index.diff_tree(target_tree, Pathspec::MATCH_ALL)?.is_empty());
            Ok(())
        })
    }
}

impl<'rcx> BitIndex<'rcx> {
    fn apply_migration(&mut self, migration: &Migration) -> BitResult<()> {
        let repo = self.repo;
        for rmrf in &migration.rmrfs {
            std::fs::remove_dir_all(repo.to_absolute_path(&rmrf.path))?;
            self.remove_directory(rmrf.path)?;
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
                    .open(&path)?;
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
        baseline: impl BitTreeIterator,
        target: impl BitTreeIterator,
    ) -> BitResult<Self> {
        MigrationDiffer::default().build_diff(baseline, target)
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
