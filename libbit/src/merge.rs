use crate::checkout::CheckoutOpts;
use crate::error::{BitError, BitResult};
use crate::index::{BitIndex, BitIndexEntry, Conflicts, MergeStage};
use crate::iter::{BitEntry, BitIterator, BitTreeIterator};
use crate::obj::{BitObject, Commit, CommitMessage, MutableBlob, Oid, TreeEntry};
use crate::peel::Peel;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use crate::xdiff;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt::{self, Display, Formatter};
use std::io::Write;

pub type ConflictStyle = diffy::ConflictStyle;

impl<'rcx> BitRepo<'rcx> {
    pub fn merge_base(self, a: Oid, b: Oid) -> BitResult<Option<&'rcx Commit<'rcx>>> {
        let commit_a = a.peel(self)?;
        let commit_b = b.peel(self)?;
        commit_a.find_merge_base(commit_b)
    }

    pub fn merge_bases(self, a: Oid, b: Oid) -> BitResult<Vec<&'rcx Commit<'rcx>>> {
        a.peel(self)?.find_merge_bases(b.peel(self)?)
    }

    pub fn merge_ref(self, their_head_ref: BitRef) -> BitResult<MergeResults> {
        self.with_index_mut(|index| index.merge(their_head_ref))
    }

    pub fn merge(self, their_head: &Revspec) -> BitResult<MergeResults> {
        self.merge_ref(self.resolve_rev(their_head)?)
    }
}

impl<'rcx> BitIndex<'rcx> {
    pub fn merge(&mut self, their_head_ref: BitRef) -> BitResult<MergeResults> {
        MergeCtxt::new(self, their_head_ref)?.merge()
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct MergeConflict {
    pub conflicts: Conflicts,
}

impl Display for MergeConflict {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "merge conflicts TODO formatting {:?}", self.conflicts)
    }
}

#[derive(Debug)]
struct MergeCtxt<'a, 'rcx> {
    repo: BitRepo<'rcx>,
    index: &'a mut BitIndex<'rcx>,
    // description of `their_head`
    their_head_desc: String,
    their_head_ref: BitRef,
    their_head: Oid,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MergeStrategy {
    FastForward,
    Recursive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MergeResults {
    Null,
    FastForward { to: BitRef },
    Merge(MergeSummary),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergeSummary {}

impl<'a, 'rcx> MergeCtxt<'a, 'rcx> {
    fn new(index: &'a mut BitIndex<'rcx>, their_head_ref: BitRef) -> BitResult<Self> {
        let repo = index.repo;
        let their_head = repo.fully_resolve_ref(their_head_ref)?;
        let their_head_desc = their_head_ref.short(repo);
        Ok(Self { repo, index, their_head_ref, their_head, their_head_desc })
    }

    fn merge_base_recursive(
        &mut self,
        our_head: &'rcx Commit<'rcx>,
        their_head: &'rcx Commit<'rcx>,
    ) -> BitResult<Option<&'rcx Commit<'rcx>>> {
        debug!("MergeCtxt::merge_base_recursive({}, {})", our_head.oid(), their_head.oid());
        let merge_bases = our_head.find_merge_bases(their_head)?;
        match &merge_bases[..] {
            [] => Ok(None),
            [merge_base] => Ok(Some(merge_base)),
            [a, b] => Some(self.make_virtual_base(a, b)).transpose(),
            _ => todo!("more than 2 merge bases"),
        }
    }

    fn make_virtual_base(
        &mut self,
        our_head: &'rcx Commit<'rcx>,
        their_head: &'rcx Commit<'rcx>,
    ) -> BitResult<&'rcx Commit<'rcx>> {
        debug!("MergeCtxt::make_virtual_base({}, {})", our_head.oid(), their_head.oid());
        let merge_base = self.merge_base_recursive(our_head, their_head)?;
        self.merge_commits(merge_base, our_head, their_head)?;

        debug_assert!(!self.index.has_conflicts());
        let merged_tree = self.index.virtual_write_tree()?;
        let merge_commit = self.repo.virtual_write_commit(
            merged_tree,
            CommitMessage::new_subject("generated virtual merge commit")?,
            smallvec![our_head.oid(), their_head.oid()],
        )?;

        #[cfg(test)]
        trace!(
            "MergeCtxt::make_virtual_base(..) :: merged_commit_tree = {:?}",
            self.repo.debug_tree(merge_commit.tree_oid())
        );
        Ok(merge_commit)
    }

    pub fn merge(&mut self) -> BitResult<MergeResults> {
        debug!("MergeCtxt::merge()");
        let repo = self.repo;
        let their_head = self.their_head;
        let our_head = repo.fully_resolve_head()?;
        let our_head_commit = our_head.peel(repo)?;
        let their_head_commit = their_head.peel(repo)?;
        let merge_base = self.merge_base_recursive(our_head_commit, their_head_commit)?;

        if let Some(merge_base) = merge_base {
            if merge_base.oid() == self.their_head {
                return Ok(MergeResults::Null);
            }

            if merge_base.oid() == our_head {
                self.index.checkout_tree_with_opts(their_head_commit, CheckoutOpts::default())?;
                repo.update_current_ref_for_ff_merge(self.their_head_ref)?;
                return Ok(MergeResults::FastForward { to: self.their_head_ref });
            }
        }

        self.merge_commits(merge_base, our_head_commit, their_head_commit)?;

        if self.index.has_conflicts() {
            bail!(BitError::MergeConflict(MergeConflict { conflicts: self.index.conflicts() }))
        }

        let merged_tree = self.index.write_tree()?;
        let merge_commit = self.repo.write_commit(
            merged_tree,
            CommitMessage::new_subject("todo ask user for commit message")?,
            // ordering is significant here for `--first-parent`
            // i.e. the first parent should always be our head
            smallvec![our_head, their_head],
        )?;

        self.index.force_checkout_tree(merge_commit)?;
        repo.update_current_ref_for_merge(their_head)?;

        Ok(MergeResults::Merge(MergeSummary {}))
    }

    fn merge_commits(
        &mut self,
        merge_base: Option<&'rcx Commit<'rcx>>,
        our_head_commit: &'rcx Commit<'rcx>,
        their_head_commit: &'rcx Commit<'rcx>,
    ) -> BitResult<()> {
        let repo = self.repo;
        let merge_base_tree = merge_base.map(|c| c.tree_oid()).unwrap_or(Oid::UNKNOWN);
        self.merge_from_iterators(
            repo.tree_iter(merge_base_tree).skip_trees(),
            repo.tree_iter(our_head_commit.tree_oid()).skip_trees(),
            repo.tree_iter(their_head_commit.tree_oid()).skip_trees(),
        )
    }

    /// 3-way merge the iterators and write the changes to the index
    fn merge_from_iterators(
        &mut self,
        base_iter: impl BitTreeIterator,
        a_iter: impl BitTreeIterator,
        b_iter: impl BitTreeIterator,
    ) -> BitResult<()> {
        let repo = self.repo;
        let walk =
            repo.walk_tree_iterators([Box::new(base_iter), Box::new(a_iter), Box::new(b_iter)]);
        walk.for_each(|[base, a, b]| self.merge_entries(base, a, b))?;

        Ok(())
    }

    fn merge_entries(
        &mut self,
        base: Option<BitIndexEntry>,
        a: Option<BitIndexEntry>,
        b: Option<BitIndexEntry>,
    ) -> BitResult<()> {
        debug!(
            "merge_entries(path: {:?}, {:?}, {:?}, {:?})",
            base.or(a).or(b).map(|entry| entry.path),
            base.as_ref().map(BitEntry::oid),
            a.as_ref().map(BitEntry::oid),
            b.as_ref().map(BitEntry::oid)
        );

        let repo = self.repo;

        let mut three_way_merge =
            |base: Option<BitIndexEntry>, y: BitIndexEntry, z: BitIndexEntry| {
                debug_assert_eq!(y.path, z.path);
                let path = y.path;

                let base_bytes = match base {
                    Some(b) => b.read_to_blob(repo)?.into_bytes(),
                    None => vec![],
                };

                if y.mode != z.mode {
                    todo!("mode conflict");
                }

                let full_path = repo.normalize_path(path.as_path())?;
                match xdiff::merge(
                    repo.config().conflict_style(),
                    "HEAD",
                    &self.their_head_desc,
                    &base_bytes,
                    &y.read_to_blob(repo)?,
                    &z.read_to_blob(repo)?,
                ) {
                    Ok(merged) => {
                        // write the merged file to disk
                        std::fs::File::create(&full_path)?.write_all(&merged)?;
                        self.index.add_entry_from_path(&full_path)
                    }
                    Err(conflicted) => {
                        // write the conflicted file to disk
                        std::fs::File::create(full_path)?.write_all(&conflicted)?;
                        if let Some(b) = base {
                            self.index.add_conflicted_entry(b, MergeStage::Base)?;
                        }
                        self.index.add_conflicted_entry(y, MergeStage::Left)?;
                        self.index.add_conflicted_entry(z, MergeStage::Right)?;

                        Ok(())
                    }
                }
            };

        match (base, a, b) {
            (None, None, None) => unreachable!(),
            // present in ancestor but removed in both newer commits so just remove it
            (Some(b), None, None) => {
                self.index.remove_entry(b.key());
                Ok(())
            }
            // y/z is a new file that is not present in the other two
            (None, Some(entry), None) | (None, None, Some(entry)) => self.index.add_entry(entry),
            // unchanged in relative to the base in one, but removed in the other
            (Some(b), Some(x), None) | (Some(b), None, Some(x)) if b.oid == x.oid => {
                self.index.remove_entry(x.key());
                Ok(())
            }
            // modify/delete conflict
            (Some(b), Some(y), None) => {
                self.index.add_conflicted_entry(b, MergeStage::Base)?;
                self.index.add_conflicted_entry(y, MergeStage::Left)
            }
            (Some(b), None, Some(z)) => {
                self.index.add_conflicted_entry(b, MergeStage::Base)?;
                self.index.add_conflicted_entry(z, MergeStage::Right)
            }
            // merge
            (None, Some(y), Some(z)) => three_way_merge(None, y, z),
            (Some(b), Some(y), Some(z)) => three_way_merge(Some(b), y, z),
        }
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
struct CommitNode<'rcx> {
    commit: &'rcx Commit<'rcx>,
    index: usize,
}

impl<'rcx> PartialOrd for CommitNode<'rcx> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CommitNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<'rcx> std::ops::Deref for CommitNode<'rcx> {
    type Target = Commit<'rcx>;

    fn deref(&self) -> &Self::Target {
        &self.commit
    }
}

impl Eq for CommitNode<'_> {
}

impl Ord for CommitNode<'_> {
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

pub struct MergeBaseCtxt<'rcx> {
    repo: BitRepo<'rcx>,
    candidates: Vec<&'rcx Commit<'rcx>>,
    pqueue: BinaryHeap<CommitNode<'rcx>>,
    node_flags: FxHashMap<Oid, NodeFlags>,
    index: usize,
}

impl<'rcx> MergeBaseCtxt<'rcx> {
    pub fn still_interesting(&self) -> bool {
        // interesting if pqueue still contains any non-stale nodes
        // otherwise, everything will be stale from here on so we can stop
        self.pqueue.iter().any(|node| !self.node_flags[&node.oid()].contains(NodeFlags::STALE))
    }

    fn mk_node(&mut self, commit: &'rcx Commit<'rcx>) -> CommitNode<'rcx> {
        let index = self.index;
        self.index += 1;
        CommitNode { index, commit }
    }

    fn merge_bases_all(
        mut self,
        a: &'rcx Commit<'rcx>,
        b: &'rcx Commit<'rcx>,
    ) -> BitResult<Vec<&'rcx Commit<'rcx>>> {
        self.build_candidates(a, b)?;
        let node_flags = &self.node_flags;
        self.candidates.retain(|node| !node_flags[&node.oid()].contains(NodeFlags::STALE));
        // TODO I think it's possible for the candidate set at this point to still be incorrect (i.e. it include some non-BCA nodes)
        // but haven't found the cases that cause this
        Ok(self.candidates)
    }

    fn build_candidates(&mut self, a: &'rcx Commit<'rcx>, b: &'rcx Commit<'rcx>) -> BitResult<()> {
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
                if !flags.contains(NodeFlags::RESULT) {
                    assert!(
                        !flags.contains(NodeFlags::STALE),
                        "maybe need to add this to the condition above?"
                    );
                    flags.insert(NodeFlags::RESULT);
                    self.candidates.push(node.commit);
                }
                // parent nodes of a result node are stale and we can rule them out of our candidate set
                parent_flags.insert(NodeFlags::STALE);
            }

            for &parent in &parents {
                let parent = self.repo.read_obj_commit(parent)?;
                self.node_flags.entry(parent.oid()).or_default().insert(parent_flags);
                let parent_node = self.mk_node(parent);
                self.pqueue.push(parent_node);
            }
        }
        Ok(())
    }
}

impl<'rcx> Commit<'rcx> {
    fn find_merge_bases(
        &'rcx self,
        other: &'rcx Commit<'rcx>,
    ) -> BitResult<Vec<&'rcx Commit<'rcx>>> {
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
    /// Only returns a single solution even when there may be multiple valid/optimal solutions.
    // TODO
    // I'm pretty sure this function will not work in all cases (i.e. return a non-optimal solution)
    // Not sure if those cases will come up realistically though, to investigate
    pub fn find_merge_base(
        &'rcx self,
        other: &'rcx Commit<'rcx>,
    ) -> BitResult<Option<&'rcx Commit<'rcx>>> {
        let merge_bases = self.find_merge_bases(other)?;
        if merge_bases.is_empty() {
            Ok(None)
        } else {
            assert!(merge_bases.len() < 2, "TODO multiple merge bases");
            Ok(Some(merge_bases[0]))
        }
    }
}

#[cfg(test)]
mod tests;
