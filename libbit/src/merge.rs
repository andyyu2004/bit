use crate::error::BitResult;
use crate::index::{BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitIterator, BitTreeIterator};
use crate::obj::{BitObject, Commit, MutableBlob, Oid, TreeEntry};
use crate::peel::Peel;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use crate::xdiff;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::io::Write;

pub type ConflictStyle = diffy::ConflictStyle;

impl<'rcx> BitRepo<'rcx> {
    pub fn merge_base(self, a: Oid, b: Oid) -> BitResult<Commit<'rcx>> {
        let commit_a = a.peel(self)?;
        let commit_b = b.peel(self)?;
        commit_a.find_merge_base(commit_b)
    }

    pub fn merge_bases(self, a: Oid, b: Oid) -> BitResult<Vec<Commit<'rcx>>> {
        a.peel(self)?.find_merge_bases(b.peel(self)?)
    }

    pub fn merge_ref(self, their_head_ref: BitRef) -> BitResult<MergeKind> {
        MergeCtxt::new(self, their_head_ref)?.merge()
    }

    pub fn merge(self, their_head: &Revspec) -> BitResult<MergeKind> {
        self.merge_ref(self.resolve_rev(their_head)?)
    }
}

#[derive(Debug)]
struct MergeCtxt<'rcx> {
    repo: BitRepo<'rcx>,
    // description of `their_head`
    their_head_desc: String,
    their_head_ref: BitRef,
    their_head: Oid,
}

#[derive(Debug, PartialEq)]
pub enum MergeKind {
    FastForward,
    Null,
    Merge(MergeSummary),
}

#[derive(Debug, PartialEq)]
pub struct MergeSummary {}

impl<'rcx> MergeCtxt<'rcx> {
    fn new(repo: BitRepo<'rcx>, their_head_ref: BitRef) -> BitResult<Self> {
        let their_head = repo.fully_resolve_ref(their_head_ref)?;
        let their_head_desc = their_head_ref.short(repo);
        Ok(Self { repo, their_head_ref, their_head, their_head_desc })
    }

    pub fn merge(&mut self) -> BitResult<MergeKind> {
        let repo = self.repo;
        let their_head = self.their_head;
        let our_head = repo.fully_resolve_head()?;
        let our_head_commit = our_head.peel(repo)?;
        let their_head_commit = their_head.peel(repo)?;
        let merge_base = our_head_commit.find_merge_base(their_head_commit)?;

        if merge_base.oid() == self.their_head {
            return Ok(MergeKind::Null);
        }

        if merge_base.oid() == our_head {
            return Ok(MergeKind::FastForward);
        }

        let summary = self.merge_from_iterators(
            repo.tree_iter(merge_base.oid()).skip_trees(),
            repo.tree_iter(our_head).skip_trees(),
            repo.tree_iter(their_head).skip_trees(),
        )?;

        Ok(MergeKind::Merge(summary))
    }

    pub fn merge_from_iterators(
        &mut self,
        base_iter: impl BitTreeIterator,
        a_iter: impl BitTreeIterator,
        b_iter: impl BitTreeIterator,
    ) -> BitResult<MergeSummary> {
        let repo = self.repo;
        let walk = repo.walk_iterators([Box::new(base_iter), Box::new(a_iter), Box::new(b_iter)]);
        walk.for_each(|[base, a, b]| self.merge_entries(base, a, b))?;
        Ok(MergeSummary {})
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
        repo.with_index_mut(|index| {
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

                    match xdiff::merge(
                        repo.config().conflictStyle()?,
                        "HEAD",
                        &self.their_head_desc,
                        &base_bytes,
                        &y.read_to_blob(repo)?,
                        &z.read_to_blob(repo)?,
                    ) {
                        Ok(merged) => {
                            let oid = repo.write_obj(&MutableBlob::new(merged))?;
                            index.add_entry(TreeEntry { oid, path: y.path, mode: y.mode }.into())
                        }
                        Err(conflicted) => {
                            // write the conflicted file to disk
                            let full_path = repo.normalize_path(path.as_path())?;

                            if let Some(b) = base {
                                index.add_conflicted_entry(b, MergeStage::Base)?;
                            }
                            index.add_conflicted_entry(y, MergeStage::Left)?;
                            index.add_conflicted_entry(z, MergeStage::Right)?;

                            std::fs::File::create(full_path)?.write_all(&conflicted)?;
                            Ok(())
                        }
                    }
                };

            match (base, a, b) {
                (None, None, None) => unreachable!(),
                // present in ancestor but removed in both newer commits so just remove it
                (Some(b), None, None) => {
                    index.remove_entry(b.key());
                    Ok(())
                }
                // y/z is a new file that is not present in the other two
                (None, Some(entry), None) | (None, None, Some(entry)) => index.add_entry(entry),
                // unchanged in relative to the base in one, but removed in the other
                (Some(b), Some(x), None) | (Some(b), None, Some(x)) if b.oid == x.oid => {
                    index.remove_entry(x.key());
                    Ok(())
                }
                // modify/delete conflict
                (Some(b), Some(y), None) => {
                    index.add_conflicted_entry(b, MergeStage::Base)?;
                    index.add_conflicted_entry(y, MergeStage::Left)
                }
                (Some(b), None, Some(z)) => {
                    index.add_conflicted_entry(b, MergeStage::Base)?;
                    index.add_conflicted_entry(z, MergeStage::Right)
                }
                // merge
                (None, Some(y), Some(z)) => three_way_merge(None, y, z),
                (Some(b), Some(y), Some(z)) => three_way_merge(Some(b), y, z),
            }
        })
    }
}

bitflags! {
    #[derive(Default)]
    pub struct NodeFlags: u8 {
        const PARENT1 = 1 << 0;
        const PARENT2 = 1 << 1;
        const RESULT = 1 << 2;
        const STALE = 1 << 3;
    }
}

#[derive(Debug)]
pub struct Node<'rcx> {
    commit: Commit<'rcx>,
    index: usize,
}

impl<'rcx> PartialOrd for Node<'rcx> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<'rcx> std::ops::Deref for Node<'rcx> {
    type Target = Commit<'rcx>;

    fn deref(&self) -> &Self::Target {
        &self.commit
    }
}

impl Eq for Node<'_> {
}

impl Ord for Node<'_> {
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
    candidates: Vec<Commit<'rcx>>,
    pqueue: BinaryHeap<Node<'rcx>>,
    node_flags: FxHashMap<Oid, NodeFlags>,
    index: usize,
}

impl<'rcx> MergeBaseCtxt<'rcx> {
    pub fn still_interesting(&self) -> bool {
        // interesting if pqueue still contains any non-stale nodes
        // otherwise, everything will be stale from here on so we can stop
        self.pqueue.iter().any(|node| !self.node_flags[&node.oid()].contains(NodeFlags::STALE))
    }

    fn mk_node(&mut self, commit: Commit<'rcx>) -> Node<'rcx> {
        let index = self.index;
        self.index += 1;
        Node { index, commit }
    }

    fn merge_bases_all(mut self, a: Commit<'rcx>, b: Commit<'rcx>) -> BitResult<Vec<Commit<'rcx>>> {
        self.build_candidates(a, b)?;
        let node_flags = &self.node_flags;
        self.candidates.retain(|node| !node_flags[&node.oid()].contains(NodeFlags::STALE));
        // TODO I think it's possible for the candidate set at this point to still be incorrect (i.e. it include some non-BCA nodes)
        // but haven't found the cases that cause this
        Ok(self.candidates)
    }

    fn build_candidates(&mut self, a: Commit<'rcx>, b: Commit<'rcx>) -> BitResult<()> {
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
    fn find_merge_bases(self, other: Commit<'rcx>) -> BitResult<Vec<Commit<'rcx>>> {
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
    pub fn find_merge_base(self, other: Commit<'rcx>) -> BitResult<Commit<'rcx>> {
        let merge_bases = self.find_merge_bases(other)?;
        assert!(!merge_bases.is_empty(), "no merge bases found");
        assert!(merge_bases.len() < 2, "TODO multiple merge bases");
        Ok(merge_bases[0].clone())
    }
}

#[cfg(test)]
mod tests;
