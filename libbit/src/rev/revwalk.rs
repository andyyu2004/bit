use super::LazyRevspec;
use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObject, Commit, Oid};
use crate::peel::Peel;
use crate::repo::BitRepo;
use bitflags::bitflags;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashMap;
use smallvec::{smallvec, SmallVec};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::Deref;

#[derive(Debug)]
pub struct RevWalk<'rcx> {
    repo: BitRepo<'rcx>,
    // map of commit oid to their flags
    // I suppose this field name should be doubly plural
    flags: FxHashMap<Oid, CommitNodeFlags>,
    queue: BinaryHeap<CommitNode<'rcx>>,
    index: usize,
}

#[derive(Debug, PartialEq)]
struct CommitNode<'rcx> {
    commit: Commit<'rcx>,
    /// The index which it was inserted into the queue.
    /// Used to break ties when timestamps are equal
    /// Larger index means it was inserted later, so we should order
    /// by highest timestamp first, followed by lowest index.
    // Not sure this is actually required for correctness but haven't convinced myself either way
    // so keeping this for safety
    index: usize,
}

bitflags! {
    #[derive(Default)]
    struct CommitNodeFlags: u8 {
        const YIELDED = 1 << 1;
        const ENQUEUED = 1 << 2;
    }
}

impl<'rcx> PartialOrd for CommitNode<'rcx> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'rcx> Deref for CommitNode<'rcx> {
    type Target = Commit<'rcx>;

    fn deref(&self) -> &Self::Target {
        &self.commit
    }
}

// probably not an entirely sound thing to do
// necessary for the ord impl below
// might be better to newtype commit
impl Eq for CommitNode<'_> {
}

impl<'rcx> Ord for CommitNode<'rcx> {
    // we want this cmp to suit a maxheap
    // so we want the most recent (largest timestamp) commit to be >= and the smallest index to be >=
    fn cmp(&self, other: &Self) -> Ordering {
        self.committer.time.cmp(&other.committer.time).then_with(|| other.index.cmp(&self.index))
    }
}

impl<'rcx> RevWalk<'rcx> {
    pub fn new(roots: SmallVec<[Commit<'rcx>; 2]>) -> Self {
        debug_assert!(!roots.is_empty());
        let mut this = Self {
            repo: roots[0].owner(),
            flags: Default::default(),
            queue: Default::default(),
            index: 0,
        };

        roots.into_iter().for_each(|commit| this.enqueue_commit(commit));
        this
    }

    fn next_index(&mut self) -> usize {
        let index = self.index;
        self.index += 1;
        index
    }

    fn mk_node(&mut self, commit: Commit<'rcx>) -> CommitNode<'rcx> {
        CommitNode { commit, index: self.next_index() }
    }

    pub fn enqueue_commit(&mut self, commit: Commit<'rcx>) {
        let flags = self.flags.entry(commit.oid()).or_default();
        if flags.intersects(CommitNodeFlags::ENQUEUED | CommitNodeFlags::YIELDED) {
            return;
        }
        flags.insert(CommitNodeFlags::ENQUEUED);
        let node = self.mk_node(commit);
        self.queue.push(node)
    }

    pub fn walk_revspecs(repo: BitRepo<'rcx>, revspecs: &[&LazyRevspec]) -> BitResult<Self> {
        let roots = revspecs
            .iter()
            .map(|&rev| repo.fully_resolve_rev(rev)?.peel(repo))
            .collect::<Result<SmallVec<_>, _>>()?;
        Ok(Self::new(roots))
    }

    pub fn walk_revspec(repo: BitRepo<'rcx>, rev: &LazyRevspec) -> BitResult<Self> {
        let root = repo.fully_resolve_rev(rev)?.peel(repo)?;
        Ok(Self::new(smallvec![root]))
    }

    pub fn walk_commit(root: Commit<'rcx>) -> BitResult<Self> {
        Self::walk_commits(std::iter::once(root))
    }

    pub fn walk_commits(roots: impl IntoIterator<Item = Commit<'rcx>>) -> BitResult<Self> {
        let roots = roots.into_iter().collect::<SmallVec<_>>();
        ensure!(!roots.is_empty());
        Ok(Self::new(roots))
    }
}

// return all commits reachable from the roots in reverse chronological order
impl<'rcx> FallibleIterator for RevWalk<'rcx> {
    type Error = BitGenericError;
    type Item = Commit<'rcx>;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        let node = match self.queue.pop() {
            Some(node) => node,
            None => return Ok(None),
        };

        for &parent in &node.parents {
            self.enqueue_commit(self.repo.read_obj(parent)?.into_commit());
        }

        self.flags.entry(node.oid()).or_default().insert(CommitNodeFlags::YIELDED);

        Ok(Some(node.commit))
    }
}
