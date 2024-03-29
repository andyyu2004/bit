use crate::checkout::CheckoutOpts;
use crate::error::{BitError, BitResult};
use crate::fs::UniquePath;
use crate::index::{BitIndexEntry, BitIndexInner, Conflicts, MergeStage};
use crate::iter::{BitEntry, BitIterator, BitTreeIterator};
use crate::obj::{BitObject, Commit, CommitMessage, FileMode, Oid, TreeEntry};
use crate::path::BitPath;
use crate::pathspec::Pathspec;
use crate::peel::Peel;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use crate::xdiff;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Write;
use std::ops::Deref;
use std::sync::Arc;

pub type ConflictStyle = diffy::ConflictStyle;

#[derive(Debug)]
pub struct MergeOpts {
    pub no_commit: bool,
    pub no_edit: bool,
    pub no_ff: bool,
}

impl MergeOpts {
    pub const DEFAULT: Self = Self { no_edit: true, no_commit: false, no_ff: false };
    pub const NO_EDIT: Self = Self { no_edit: true, ..Self::DEFAULT };
}

impl Default for MergeOpts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl BitRepo {
    pub fn merge_base(&self, a: Oid, b: Oid) -> BitResult<Option<Arc<Commit>>> {
        let commit_a = a.peel(self)?;
        let commit_b = b.peel(self)?;
        commit_a.find_merge_base(&commit_b)
    }

    pub fn merge_bases(&self, a: Oid, b: Oid) -> BitResult<Vec<Arc<Commit>>> {
        a.peel(self)?.find_merge_bases(&b.peel(self)?)
    }

    pub fn merge(&self, their_head_ref: BitRef, opts: MergeOpts) -> BitResult<MergeResults> {
        MergeCtxt::new(self, their_head_ref, opts)?.merge()
    }

    pub fn merge_with_base(
        &self,
        their_head_ref: BitRef,
        base: Option<Arc<Commit>>,
        opts: MergeOpts,
    ) -> BitResult<MergeResults> {
        MergeCtxt::new(self, their_head_ref, opts)?.merge_with_base(base)
    }

    pub fn merge_rev(&self, their_head: &Revspec, opts: MergeOpts) -> BitResult<MergeResults> {
        self.merge(self.resolve_rev(their_head)?, opts)
    }
}

/// The conflicts that prevent a merge from occurring (not the "merge conflicts")
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct MergeConflicts {
    /// Uncommitted changes that will be overwritten by merge
    pub uncommitted: Vec<BitPath>,
}

impl Display for MergeConflicts {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "merge conflicts TODO formatting {:?}", self.uncommitted)
    }
}

#[derive(Debug)]
struct MergeCtxt {
    repo: BitRepo,
    // description of `their_head`
    their_head_desc: String,
    their_head_ref: BitRef,
    their_head: Oid,
    opts: MergeOpts,
    // The state of the index pre-merge
    initial_index_snapshot: BitIndexInner,
    uncommitted: Vec<BitPath>,
    our_head: Oid,
}

impl Deref for MergeCtxt {
    type Target = BitRepo;

    fn deref(&self) -> &Self::Target {
        &self.repo
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MergeStrategy {
    FastForward,
    Recursive,
}

#[derive(Debug, PartialEq)]
pub enum MergeResults {
    Null,
    Conflicts(Conflicts),
    FastForward { from: Oid, to: Oid },
    Merge(MergeSummary),
}

impl MergeResults {
    #[cfg(test)]
    pub fn into_conflicts(self) -> Conflicts {
        match self {
            MergeResults::Conflicts(conflicts) => conflicts,
            _ => panic!("expected merge to conflict"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergeSummary {}

impl MergeCtxt {
    fn new(repo: &BitRepo, their_head_ref: BitRef, opts: MergeOpts) -> BitResult<Self> {
        let our_head = repo.fully_resolve_head()?;

        let their_head = repo.fully_resolve_ref(their_head_ref)?;
        let their_head_desc = their_head_ref.short();

        Ok(Self {
            repo: repo.clone(),
            our_head,
            their_head_ref,
            their_head,
            their_head_desc,
            opts,
            initial_index_snapshot: repo.index()?.clone(),
            uncommitted: Default::default(),
        })
    }

    fn merge_base_recursive(
        &mut self,
        our_head: &Arc<Commit>,
        their_head: &Arc<Commit>,
    ) -> BitResult<Option<Arc<Commit>>> {
        debug!("MergeCtxt::merge_base_recursive({}, {})", our_head.oid(), their_head.oid());
        let merge_bases = our_head.find_merge_bases(their_head)?;
        match &merge_bases[..] {
            [] => Ok(None),
            [merge_base] => Ok(Some(merge_base.clone())),
            [a, b] => self.make_virtual_base(a, b)?.peel(self).map(Some),
            _ => todo!("more than 2 merge bases"),
        }
    }

    fn make_virtual_base(
        &mut self,
        our_head: &Arc<Commit>,
        their_head: &Arc<Commit>,
    ) -> BitResult<Oid> {
        debug!("MergeCtxt::make_virtual_base({}, {})", our_head.oid(), their_head.oid());
        let merge_base = self.merge_base_recursive(our_head, their_head)?;
        self.merge_commits(merge_base, our_head, their_head)?;

        let mut index = self.repo.index_mut()?;
        debug_assert!(!index.has_conflicts());
        let merged_tree = index.write_tree()?;
        drop(index);

        let merge_commit = self.repo.write_commit(
            merged_tree,
            smallvec![our_head.oid(), their_head.oid()],
            CommitMessage::new_subject("generated virtual merge commit")?,
        )?;

        #[cfg(test)]
        trace!(
            "MergeCtxt::make_virtual_base(..) :: merged_commit_tree = {:?}",
            self.repo.debug_tree(merge_commit)
        );
        Ok(merge_commit)
    }

    fn check_unstaged_changes(&mut self) -> BitResult<()> {
        let diff = self.repo.diff_index_worktree(Pathspec::MATCH_ALL)?;
        self.uncommitted.extend(diff.iter_paths());
        Ok(())
    }

    fn check_staged_changes(&mut self) -> BitResult<()> {
        let diff = self.repo.diff_head_index(Pathspec::MATCH_ALL)?;
        self.uncommitted.extend(diff.iter_paths());
        Ok(())
    }

    // most of these checks can probably be relaxed but it's unclear whether it's worth the effort
    // we just disallow any unstaged and staged changes (but non-conflicting untracked is allowed)
    fn pre_merge_checks(&mut self) -> BitResult<()> {
        self.check_unstaged_changes()?;
        self.check_staged_changes()?;
        Ok(())
    }

    pub fn merge(mut self) -> BitResult<MergeResults> {
        debug!("MergeCtxt::merge()");
        let merge_base = self
            .merge_base_recursive(&self.our_head_commit()?, &self.their_head.peel(&self.repo)?)?;
        self.merge_with_base(merge_base)
    }

    fn our_head_commit(&self) -> BitResult<Arc<Commit>> {
        self.our_head.peel(&self.repo)
    }

    fn their_head_commit(&self) -> BitResult<Arc<Commit>> {
        self.their_head.peel(&self.repo)
    }

    pub fn merge_with_base(mut self, merge_base: Option<Arc<Commit>>) -> BitResult<MergeResults> {
        debug!(
            "MergeCtxt::merge_with_base(merge_base: {:?})",
            merge_base.as_ref().map(|b| b.oid())
        );

        self.pre_merge_checks()?;

        if let Some(merge_base) = &merge_base {
            if merge_base.oid() == self.their_head {
                return Ok(MergeResults::Null);
            }

            if !self.opts.no_ff && merge_base.oid() == self.our_head {
                self.checkout_tree_with_opts(
                    self.their_head.peel(&self.repo)?,
                    CheckoutOpts::default(),
                )?;
                self.update_current_ref_for_ff_merge(self.their_head_ref)?;
                return Ok(MergeResults::FastForward { from: self.our_head, to: self.their_head });
            }
        }

        self.merge_commits(merge_base, &self.our_head_commit()?, &self.their_head_commit()?)?;

        if !self.uncommitted.is_empty() {
            // Restore index to pre-merge snapshot and check it out to also restore the worktree to inital pre-merge state
            // We perform a safe checkout as we don't want to overwrite unstaged files
            **self.repo.index_mut()? = self.initial_index_snapshot.clone();
            self.repo.checkout_index(CheckoutOpts::default())?;
            bail!(BitError::MergeConflict(MergeConflicts { uncommitted: self.uncommitted }))
        }

        if self.index()?.has_conflicts() {
            return Ok(MergeResults::Conflicts(self.index()?.conflicts()));
        }

        if !self.opts.no_commit {
            let message = self
                .opts
                .no_edit
                .then(|| format!("Merge commit `{}` into HEAD", self.their_head_ref));
            let merged_tree = self.index_mut()?.write_tree()?;
            let merge_commit = self.repo.commit_tree(
                merged_tree,
                // ordering is significant here for `--first-parent`
                // i.e. the first parent should always be our head
                smallvec![self.our_head, self.their_head],
                message,
            )?;

            self.update_current_ref_for_merge(merge_commit)?;
        }

        Ok(MergeResults::Merge(MergeSummary {}))
    }

    fn merge_commits(
        &mut self,
        merge_base: Option<Arc<Commit>>,
        our_head_commit: &Arc<Commit>,
        their_head_commit: &Arc<Commit>,
    ) -> BitResult<()> {
        let merge_base_tree = merge_base.map(|c| c.tree_oid()).unwrap_or(Oid::UNKNOWN);
        self.merge_trees(merge_base_tree, our_head_commit.tree_oid(), their_head_commit.tree_oid())
    }

    pub fn merge_trees(&mut self, base_tree: Oid, our_tree: Oid, their_tree: Oid) -> BitResult<()> {
        self.merge_from_iterators(
            self.tree_iter(base_tree),
            self.tree_iter(our_tree),
            self.tree_iter(their_tree),
        )
    }

    /// 3-way merge the iterators and write the changes to the index
    fn merge_from_iterators(
        &mut self,
        base_iter: impl BitTreeIterator,
        a_iter: impl BitTreeIterator,
        b_iter: impl BitTreeIterator,
    ) -> BitResult<()> {
        let repo = self.repo.clone();
        let walk =
            repo.walk_tree_iterators([Box::new(base_iter), Box::new(a_iter), Box::new(b_iter)]);
        walk.for_each(|[base, a, b]| self.merge_entries(base, a, b))?;
        Ok(())
    }

    // this is pretty similar to tree_diff but dissimilar enough to warrant doing it separately I think
    fn diff_base_to_other(
        &mut self,
        base: Option<BitIndexEntry>,
        other: Option<BitIndexEntry>,
    ) -> Option<MergeDiffEntry> {
        match (base, other) {
            (None, None) => None,
            (None, Some(other)) => Some(self.other_only(other)),
            (Some(base), None) => Some(self.base_only(base)),
            (Some(base), Some(other)) => match base.diff_cmp(&other) {
                Ordering::Less => Some(self.base_only(base)),
                Ordering::Greater => Some(self.other_only(other)),
                Ordering::Equal => {
                    assert!(base.oid().is_known());
                    assert!(other.oid().is_known());
                    match (base.mode(), other.mode()) {
                        (FileMode::GITLINK, _) | (_, FileMode::GITLINK) => todo!("submodules"),
                        (FileMode::TREE, FileMode::TREE) if base.oid() == other.oid() =>
                            Some(MergeDiffEntry::UnmodifiedTree(other)),
                        (FileMode::TREE, FileMode::TREE) =>
                            Some(MergeDiffEntry::ModifiedTree(other)),
                        (FileMode::TREE, _) => Some(MergeDiffEntry::TreeToBlob(other)),
                        (_, FileMode::TREE) => Some(MergeDiffEntry::BlobToTree(other)),
                        _ if base.oid() == other.oid() =>
                            Some(MergeDiffEntry::UnmodifiedBlob(other)),
                        _ => Some(MergeDiffEntry::ModifiedBlob(other)),
                    }
                }
            },
        }
    }

    fn base_only(&mut self, base: BitIndexEntry) -> MergeDiffEntry {
        match base.mode() {
            FileMode::REG | FileMode::EXEC | FileMode::LINK => MergeDiffEntry::DeletedBlob(base),
            FileMode::TREE => MergeDiffEntry::DeletedTree(base),
            FileMode::GITLINK => todo!(),
        }
    }

    fn other_only(&mut self, other: BitIndexEntry) -> MergeDiffEntry {
        match other.mode() {
            FileMode::REG | FileMode::EXEC | FileMode::LINK => MergeDiffEntry::CreatedBlob(other),
            FileMode::TREE => MergeDiffEntry::CreatedTree(other),
            FileMode::GITLINK => todo!(),
        }
    }

    fn merge_entries(
        &mut self,
        base: Option<BitIndexEntry>,
        ours: Option<BitIndexEntry>,
        theirs: Option<BitIndexEntry>,
    ) -> BitResult<()> {
        debug!(
            "MergeCtxt::merge_entries(base: {:?}, ours: {:?}, theirs: {:?})",
            base.map(TreeEntry::from),
            ours.map(TreeEntry::from),
            theirs.map(TreeEntry::from)
        );
        let base_to_ours = self.diff_base_to_other(base, ours);
        let base_to_theirs = self.diff_base_to_other(base, theirs);
        match (base_to_ours, base_to_theirs) {
            (None, None) => Ok(()),
            (None, Some(base_to_theirs)) => self.base_to_theirs_only(base_to_theirs),
            (Some(base_to_ours), None) => self.base_to_ours_only(base_to_ours),
            (Some(base_to_ours), Some(base_to_theirs)) =>
                self.merge_entry(base, base_to_ours, base_to_theirs),
        }
    }

    fn base_to_ours_only(&mut self, base_to_ours: MergeDiffEntry) -> BitResult<()> {
        debug!("MergeCtxt::base_to_ours_only(base_to_ours: {:?})", base_to_ours);
        match base_to_ours {
            MergeDiffEntry::CreatedBlob(entry) => self.repo.index_mut()?.add_entry(entry),
            MergeDiffEntry::CreatedTree(_) => Ok(()),
            _ => unreachable!(),
        }
    }

    fn base_to_theirs_only(&mut self, base_to_theirs: MergeDiffEntry) -> BitResult<()> {
        debug!("MergeCtxt::base_to_theirs_only(base_to_theirs: {:?})", base_to_theirs,);
        let repo = self.repo.clone();
        match base_to_theirs {
            MergeDiffEntry::CreatedBlob(entry) => {
                // Its possible for there to be an untracked file that clashes with a new file in their HEAD
                if repo.path_exists(entry.path())? {
                    self.uncommitted.push(entry.path());
                } else {
                    repo.index_mut()?.write_and_add_blob(entry)?;
                };
                Ok(())
            }
            MergeDiffEntry::CreatedTree(tree) => repo.mkdir(tree.path()),
            _ => unreachable!(),
        }
    }

    fn write_their_conflicted(&self, theirs: &BitIndexEntry) -> BitResult<()> {
        let moved_path = UniquePath::make(
            self.repo.clone(),
            format!("{}~{}", theirs.path(), self.their_head_desc),
        )?;
        theirs.write_to_disk_at(&self.repo, moved_path)
    }

    fn mv_our_conflicted(&mut self, path: BitPath) -> BitResult<()> {
        let moved_path = UniquePath::make(self.repo.clone(), format!("{path}~HEAD"))?;
        self.repo.mv(path, moved_path)
    }

    fn merge_entry(
        &mut self,
        base: Option<BitIndexEntry>,
        base_to_ours: MergeDiffEntry,
        base_to_theirs: MergeDiffEntry,
    ) -> BitResult<()> {
        debug!(
            "MergeCtxt::merge_entries(base: {:?}, ours: {:?}, theirs: {:?})",
            base.as_ref().map(TreeEntry::from),
            base_to_ours,
            base_to_theirs,
        );

        macro_rules! index_mut {
            () => {{ self.repo.index_mut()? }};
        }

        let three_way_merge =
            |base: Option<BitIndexEntry>, ours: BitIndexEntry, theirs: BitIndexEntry| {
                debug_assert_eq!(ours.path, theirs.path);
                let path = ours.path;

                let base_bytes = match base {
                    Some(b) => b.read_to_bytes(&self.repo)?,
                    None => vec![],
                };

                if ours.mode != theirs.mode {
                    todo!("mode conflict");
                }

                let full_path = self.repo.normalize_path(path.as_path())?;
                let mut file = std::fs::OpenOptions::new()
                    .create(false)
                    .read(false)
                    .write(true)
                    .open(&full_path)?;
                match xdiff::merge(
                    self.repo.config().conflict_style(),
                    "HEAD",
                    &self.their_head_desc,
                    &base_bytes,
                    &ours.read_to_bytes(&self.repo)?,
                    &theirs.read_to_bytes(&self.repo)?,
                ) {
                    Ok(merged) => {
                        // write the merged file to disk
                        file.write_all(&merged)?;
                        index_mut!().add_entry_from_path(&full_path)
                    }
                    Err(conflicted) => {
                        // write the conflicted file to disk
                        file.write_all(&conflicted)?;
                        if let Some(b) = base {
                            index_mut!().add_conflicted_entry(b, MergeStage::BASE)?;
                        }
                        index_mut!().add_conflicted_entry(ours, MergeStage::OURS)?;
                        index_mut!().add_conflicted_entry(theirs, MergeStage::THEIRS)?;

                        Ok(())
                    }
                }
            };

        match (base_to_ours, base_to_theirs) {
            (MergeDiffEntry::DeletedBlob(entry), MergeDiffEntry::DeletedBlob(_)) =>
                index_mut!().remove_entry(entry.key()),
            // CONFLICT (modify/delete): dir/bar deleted in theirs and modified in HEAD. Version HEAD of dir/bar left in tree.
            // Automatic merge failed; fix conflicts and then commit the result.
            (MergeDiffEntry::DeletedBlob(_), MergeDiffEntry::ModifiedBlob(theirs)) => {
                index_mut!().add_conflicted_entry(base.unwrap(), MergeStage::BASE)?;
                index_mut!().add_conflicted_entry(theirs, MergeStage::THEIRS)?;
                theirs.write_to_disk(&self.repo)?;
            }
            (MergeDiffEntry::ModifiedBlob(ours), MergeDiffEntry::DeletedBlob(_)) => {
                index_mut!().add_conflicted_entry(base.unwrap(), MergeStage::BASE)?;
                index_mut!().add_conflicted_entry(ours, MergeStage::OURS)?;
            }
            (MergeDiffEntry::DeletedBlob(ours), MergeDiffEntry::UnmodifiedBlob(_)) =>
                index_mut!().remove_entry(ours.key()),
            (MergeDiffEntry::CreatedBlob(ours), MergeDiffEntry::CreatedBlob(theirs)) =>
                three_way_merge(base, ours, theirs)?,
            (MergeDiffEntry::ModifiedBlob(ours), MergeDiffEntry::ModifiedBlob(theirs)) =>
                three_way_merge(base, ours, theirs)?,
            (MergeDiffEntry::ModifiedBlob(entry), MergeDiffEntry::UnmodifiedBlob(_)) =>
                index_mut!().add_entry(entry)?,
            (MergeDiffEntry::ModifiedBlob(ours), MergeDiffEntry::BlobToTree(tree)) => {
                // TODO
                // example git output for a case like `test_merge_modified_file_to_tree()`
                // Adding foo/bar
                // CONFLICT (modify/delete): foo deleted in theirs and modified in HEAD. Version HEAD of foo left in tree at foo~HEAD.
                // Automatic merge failed; fix conflicts and then commit the result.
                index_mut!().add_conflicted_entry(base.unwrap(), MergeStage::BASE)?;
                index_mut!().add_conflicted_entry(ours, MergeStage::OURS)?;
                self.mv_our_conflicted(ours.path())?;
                self.repo.mkdir(tree.path())?
            }
            (MergeDiffEntry::BlobToTree(..), MergeDiffEntry::ModifiedBlob(theirs)) => {
                index_mut!().add_conflicted_entry(base.unwrap(), MergeStage::BASE)?;
                index_mut!().add_conflicted_entry(theirs, MergeStage::THEIRS)?;
                self.write_their_conflicted(&theirs)?;
            }
            (MergeDiffEntry::ModifiedTree(_), MergeDiffEntry::TreeToBlob(theirs)) => {
                index_mut!().add_conflicted_entry(theirs, MergeStage::THEIRS)?;
                self.write_their_conflicted(&theirs)?;
            }
            (MergeDiffEntry::TreeToBlob(ours), MergeDiffEntry::ModifiedTree(_)) => {
                index_mut!().add_conflicted_entry(ours, MergeStage::OURS)?;
                self.mv_our_conflicted(ours.path())?;
            }
            (MergeDiffEntry::UnmodifiedTree(tree), MergeDiffEntry::DeletedTree(_)) => {
                let path = tree.path();
                index_mut!().remove_directory(path)?;
                self.repo.rmdir_all(path)?;
            }
            (MergeDiffEntry::UnmodifiedBlob(_), MergeDiffEntry::ModifiedBlob(theirs)) =>
                index_mut!().update_blob(theirs)?,
            (MergeDiffEntry::UnmodifiedBlob(_), MergeDiffEntry::BlobToTree(tree)) => {
                let path = tree.path();
                self.repo.rm(path)?;
                self.repo.mkdir(path)?;
            }
            (MergeDiffEntry::CreatedBlob(ours), MergeDiffEntry::CreatedTree(_)) => {
                index_mut!().add_conflicted_entry(ours, MergeStage::OURS)?;
                self.mv_our_conflicted(ours.path())?;
            }
            (MergeDiffEntry::CreatedTree(_), MergeDiffEntry::CreatedBlob(theirs)) => {
                index_mut!().add_conflicted_entry(theirs, MergeStage::THEIRS)?;
                self.write_their_conflicted(&theirs)?;
            }
            (MergeDiffEntry::DeletedTree(entry), MergeDiffEntry::DeletedTree(_)) =>
                index_mut!().remove_directory(entry.path())?,
            (MergeDiffEntry::UnmodifiedTree(ours), MergeDiffEntry::TreeToBlob(theirs)) => {
                self.repo.rmdir_all(ours.path())?;
                theirs.write_to_disk(&self.repo)?;
                index_mut!().add_entry(theirs)?;
            }
            (MergeDiffEntry::UnmodifiedBlob(_), MergeDiffEntry::DeletedBlob(theirs)) => {
                // The path might already be deleted due to the case above removing the entire directory
                // We handle this by just ignoring the error (not perfect but should be practically fine)
                let _ = self.repo.rm(theirs.path());
                index_mut!().remove_entry(theirs.key());
            }
            (MergeDiffEntry::DeletedTree(_), MergeDiffEntry::TreeToBlob(theirs)) => {
                theirs.write_to_disk(&self.repo)?;
                index_mut!().add_entry(theirs)?;
            }
            (MergeDiffEntry::TreeToBlob(ours), MergeDiffEntry::TreeToBlob(theirs)) =>
                three_way_merge(None, ours, theirs)?,
            (MergeDiffEntry::ModifiedTree(_), MergeDiffEntry::ModifiedTree(_))
            | (MergeDiffEntry::CreatedTree(_), MergeDiffEntry::CreatedTree(_))
            | (MergeDiffEntry::BlobToTree(_), MergeDiffEntry::UnmodifiedBlob(_))
            | (MergeDiffEntry::BlobToTree(_), MergeDiffEntry::BlobToTree(_))
            | (MergeDiffEntry::ModifiedTree(_), MergeDiffEntry::UnmodifiedTree(_))
            | (MergeDiffEntry::UnmodifiedTree(_), MergeDiffEntry::ModifiedTree(_))
            | (MergeDiffEntry::DeletedTree(_), MergeDiffEntry::ModifiedTree(_))
            | (MergeDiffEntry::UnmodifiedTree(_), MergeDiffEntry::UnmodifiedTree(_))
            | (MergeDiffEntry::BlobToTree(_), MergeDiffEntry::DeletedBlob(_))
            | (MergeDiffEntry::UnmodifiedBlob(_), MergeDiffEntry::UnmodifiedBlob(_))
            | (MergeDiffEntry::DeletedBlob(_), MergeDiffEntry::BlobToTree(_))
            | (MergeDiffEntry::ModifiedTree(_), MergeDiffEntry::DeletedTree(_))
            | (MergeDiffEntry::TreeToBlob(_), MergeDiffEntry::UnmodifiedTree(_))
            | (MergeDiffEntry::TreeToBlob(_), MergeDiffEntry::DeletedTree(_))
            | (MergeDiffEntry::DeletedTree(_), MergeDiffEntry::UnmodifiedTree(_)) => {}
            _ => unreachable!("the remaining cases should be impossible"),
        }

        Ok(())
    }
}

// The entries present in each variant represents the "new" entry
// i.e. post modification/typechange
enum MergeDiffEntry {
    DeletedBlob(BitIndexEntry),
    CreatedBlob(BitIndexEntry),
    ModifiedBlob(BitIndexEntry),
    ModifiedTree(BitIndexEntry),
    UnmodifiedBlob(BitIndexEntry),
    UnmodifiedTree(BitIndexEntry),
    DeletedTree(BitIndexEntry),
    CreatedTree(BitIndexEntry),
    BlobToTree(BitIndexEntry),
    TreeToBlob(BitIndexEntry),
}

impl Debug for MergeDiffEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeletedBlob(entry) =>
                f.debug_tuple("DeletedBlob").field(&TreeEntry::from(entry)).finish(),
            Self::CreatedBlob(entry) =>
                f.debug_tuple("CreatedBlob").field(&TreeEntry::from(entry)).finish(),
            Self::ModifiedBlob(entry) =>
                f.debug_tuple("ModifiedBlob").field(&TreeEntry::from(entry)).finish(),
            Self::ModifiedTree(entry) =>
                f.debug_tuple("ModifiedTree").field(&TreeEntry::from(entry)).finish(),
            Self::UnmodifiedBlob(entry) =>
                f.debug_tuple("UnmodifiedBlob").field(&TreeEntry::from(entry)).finish(),
            Self::UnmodifiedTree(entry) =>
                f.debug_tuple("UnmodifiedTree").field(&TreeEntry::from(entry)).finish(),
            Self::DeletedTree(entry) =>
                f.debug_tuple("DeletedTree").field(&TreeEntry::from(entry)).finish(),
            Self::CreatedTree(entry) =>
                f.debug_tuple("CreatedTree").field(&TreeEntry::from(entry)).finish(),
            Self::BlobToTree(entry) =>
                f.debug_tuple("BlobToTree").field(&TreeEntry::from(entry)).finish(),
            Self::TreeToBlob(entry) =>
                f.debug_tuple("TreeToBlob").field(&TreeEntry::from(entry)).finish(),
        }
    }
}

impl MergeDiffEntry {
    pub fn entry(&self) -> BitIndexEntry {
        match *self {
            MergeDiffEntry::DeletedBlob(entry) => entry,
            MergeDiffEntry::CreatedBlob(entry) => entry,
            MergeDiffEntry::ModifiedBlob(entry) => entry,
            MergeDiffEntry::ModifiedTree(entry) => entry,
            MergeDiffEntry::UnmodifiedBlob(entry) => entry,
            MergeDiffEntry::UnmodifiedTree(entry) => entry,
            MergeDiffEntry::DeletedTree(entry) => entry,
            MergeDiffEntry::CreatedTree(entry) => entry,
            MergeDiffEntry::BlobToTree(entry) => entry,
            MergeDiffEntry::TreeToBlob(entry) => entry,
        }
    }
}

impl BitEntry for MergeDiffEntry {
    fn oid(&self) -> Oid {
        self.entry().oid()
    }

    fn path(&self) -> BitPath {
        self.entry().path()
    }

    fn mode(&self) -> FileMode {
        self.entry().mode()
    }
}

bitflags! {
    #[derive(Default)]
    struct NodeFlags: u8 {
        const PARENT1 = 1 << 0;
        const PARENT2 = 1 << 1;
        const RESULT = 1 << 2;
        const STALE = 1 << 3;
    }
}

#[derive(Debug)]
struct CommitNode {
    commit: Arc<Commit>,
    index: usize,
}

impl PartialOrd for CommitNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CommitNode {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl std::ops::Deref for CommitNode {
    type Target = Commit;

    fn deref(&self) -> &Self::Target {
        &self.commit
    }
}

impl Eq for CommitNode {
}

impl Ord for CommitNode {
    // we want this cmp to suit a maxheap
    // so we want the most recent (largest timestamp) commit to be >= and the smallest index to be >=
    fn cmp(&self, other: &Self) -> Ordering {
        self.commit
            .committer
            .time
            .cmp(&other.commit.committer.time)
            .then_with(|| other.index.cmp(&self.index))
            .then_with(|| bug!("index should be unique"))
    }
}

pub struct MergeBaseCtxt {
    repo: BitRepo,
    candidates: Vec<Arc<Commit>>,
    pqueue: BinaryHeap<CommitNode>,
    node_flags: FxHashMap<Oid, NodeFlags>,
    index: usize,
}

impl MergeBaseCtxt {
    pub fn still_interesting(&self) -> bool {
        // interesting if pqueue still contains any non-stale nodes
        // otherwise, everything will be stale from here on so we can stop
        self.pqueue.iter().any(|node| !self.node_flags[&node.oid()].contains(NodeFlags::STALE))
    }

    fn mk_node(&mut self, commit: Arc<Commit>) -> CommitNode {
        let index = self.index;
        self.index += 1;
        CommitNode { index, commit }
    }

    fn merge_bases_all(mut self, a: &Arc<Commit>, b: &Arc<Commit>) -> BitResult<Vec<Arc<Commit>>> {
        self.build_candidates(a.clone(), b.clone())?;
        let node_flags = &self.node_flags;
        self.candidates.retain(|node| !node_flags[&node.oid()].contains(NodeFlags::STALE));
        // TODO I think it's possible for the candidate set at this point to still be incorrect (i.e. it include some non-BCA nodes)
        // but haven't found the cases that cause this
        Ok(self.candidates)
    }

    fn build_candidates(&mut self, a: Arc<Commit>, b: Arc<Commit>) -> BitResult<()> {
        let mut push_init = |commit, flags| {
            let node = self.mk_node(commit);
            self.node_flags.entry(node.oid()).or_default().insert(flags);
            self.pqueue.push(node);
        };

        push_init(a, NodeFlags::PARENT1);
        push_init(b, NodeFlags::PARENT2);

        while self.still_interesting() {
            let node = match self.pqueue.pop() {
                Some(node) => node,
                None => break,
            };

            let flags = self.node_flags.get_mut(&node.oid()).unwrap();
            let parents = node.commit.parents.clone();
            // unset the result bit, as we don't want to propogate the result flag
            let mut parent_flags = *flags & !NodeFlags::RESULT;

            if flags.contains(NodeFlags::PARENT1 | NodeFlags::PARENT2) {
                // parent nodes of a potential result node are stale and we can rule them out of our candidate set
                parent_flags.insert(NodeFlags::STALE);
                // add to the candidate set only if it is neither a result or stale
                if !flags.intersects(NodeFlags::RESULT | NodeFlags::STALE) {
                    flags.insert(NodeFlags::RESULT);
                    self.candidates.push(node.commit);
                }
            }

            for &parent in &parents {
                let pflags = self.node_flags.entry(parent).or_default();
                if *pflags == parent_flags {
                    continue;
                }
                let parent = self.repo.read_obj_commit(parent)?;
                pflags.insert(parent_flags);
                let parent_node = self.mk_node(parent);
                self.pqueue.push(parent_node);
            }
        }
        Ok(())
    }
}

impl Commit {
    fn find_merge_bases(self: &Arc<Self>, other: &Arc<Commit>) -> BitResult<Vec<Arc<Commit>>> {
        MergeBaseCtxt {
            repo: self.owner(),
            candidates: Default::default(),
            node_flags: Default::default(),
            pqueue: Default::default(),
            index: Default::default(),
        }
        .merge_bases_all(self, other)
    }

    /// Returns lowest common ancestor found.
    /// If there are multiple candidates then the first is returned
    pub fn find_merge_base(
        self: &Arc<Self>,
        other: &Arc<Commit>,
    ) -> BitResult<Option<Arc<Commit>>> {
        let mut merge_bases = self.find_merge_bases(other)?;
        if merge_bases.is_empty() { Ok(None) } else { Ok(Some(merge_bases.swap_remove(0))) }
    }
}

#[cfg(test)]
mod tests;
