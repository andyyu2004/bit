use crate::diff::{TreeDiffBuilder, TreeDiffer, TreeEntriesConsumer};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::obj::{FileMode, Oid, TreeEntry};
use crate::repo::BitRepo;

impl<'rcx> BitRepo<'rcx> {
    /// update the worktree to match the tree represented by `target`
    pub fn checkout_tree(&self, target_tree: Oid) -> BitResult<()> {
        let baseline = self.head_tree_iter()?;
        let target = self.tree_iter(target_tree);
        // let workdir = self.with_index(|index| index.worktree_iter())?;

        let migration = MigrationDiffer::default().build_diff(baseline, target);

        Ok(())
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
mod tests;
