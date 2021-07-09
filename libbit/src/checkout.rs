use crate::diff::{TreeDiffBuilder, TreeDiffer, TreeEntriesConsumer};
use crate::error::BitResult;
use crate::index::{BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitTreeIterator};
use crate::obj::{BitObject, Commit, FileMode, TreeEntry};
use crate::pathspec::Pathspec;
use crate::refs::{BitRef, RefUpdateCause};
use crate::repo::BitRepo;
use crate::rev::LazyRevspec;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::prelude::PermissionsExt;

impl<'rcx> BitRepo<'rcx> {
    pub fn checkout_rev(self, rev: &LazyRevspec) -> BitResult<()> {
        let commit_oid = self.resolve_rev(rev)?;
        let commit = self.read_obj(commit_oid)?.into_commit();
        self.checkout_commit(&commit)
    }

    /// checkout the commit
    /// - updates the worktree to match the tree represented by the tree of the commit
    /// - updates HEAD
    pub fn checkout_commit(self, commit: &Commit) -> BitResult<()> {
        let target_tree = commit.tree;
        let status = self.status(Pathspec::MATCH_ALL)?;
        // only allow checkout on fully clean states for now
        if !status.is_empty() {
            bail!("cannot checkout: unclean state")
        }

        let baseline = self.head_tree_iter()?;
        let target = self.tree_iter(target_tree);

        // let workdir = self.with_index(|index| index.worktree_iter())?;

        let migration = Migration::generate(baseline, target)?;
        self.apply_migration(&migration)?;

        let new_ref = BitRef::Direct(commit.oid());
        self.update_head(
            new_ref,
            RefUpdateCause::Checkout { from: self.read_head()?, to: new_ref },
        )?;

        debug_assert!(self.status(Pathspec::MATCH_ALL)?.is_empty());
        Ok(())
    }

    fn apply_migration(self, migration: &Migration) -> BitResult<()> {
        self.with_index_mut(|index| {
            for rmrf in &migration.rmrfs {
                std::fs::remove_dir_all(self.to_absolute_path(&rmrf.path))?;
                index.remove_directory(rmrf.path)?;
            }

            for rm in &migration.rms {
                std::fs::remove_file(self.to_absolute_path(&rm.path))?;
                index.remove_entry((rm.path, MergeStage::None));
            }

            for mkdir in &migration.mkdirs {
                std::fs::create_dir(self.to_absolute_path(&mkdir.path))?;
            }

            for create in &migration.creates {
                let path = self.to_absolute_path(&create.path);
                let bytes = create.read_to_bytes(self)?;
                let mut file = std::fs::File::with_options()
                    .create_new(true)
                    .read(false)
                    .write(true)
                    .open(&path)?;
                std::fs::set_permissions(&path, Permissions::from_mode(create.mode.as_u32()))?;
                file.write_all(&bytes)?;
                index.add_entry(BitIndexEntry::from_path(self, &path)?)?;
            }
            Ok(())
        })
    }
}

pub struct CheckoutCtxt {}

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
