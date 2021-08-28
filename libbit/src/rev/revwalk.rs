use super::Revspec;
use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObject, Commit, Oid};
use crate::peel::Peel;
use crate::repo::BitRepo;
use crate::signature::BitTime;
use bitflags::bitflags;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashMap;
use smallvec::{smallvec, SmallVec};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct RevWalk<'rcx> {
    repo: BitRepo<'rcx>,
    // determine's the order in which nodes will be yielded
    // the node itself can be looked up inside the map using the oid field
    nodes: FxHashMap<Oid, CommitNode<'rcx>>,
    pqueue: BinaryHeap<CommitNodeOrdering>,
    index: usize,
}

// determines the ordering of a commit node, but is a separate entity from the node
#[derive(Eq, Debug, Clone)]
pub(crate) struct CommitNodeOrdering {
    time: BitTime,
    index: usize,
    oid: Oid,
}

impl PartialEq for CommitNodeOrdering {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(&other) == Ordering::Equal
    }
}

impl PartialOrd for CommitNodeOrdering {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CommitNodeOrdering {
    // we want this cmp to suit a maxheap
    // so we want the most recent (largest timestamp) commit to be >= and the smallest index to be >=
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then_with(|| other.index.cmp(&self.index))
            .then_with(|| bug!("index should be unique"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitNode<'rcx> {
    pub commit: Commit<'rcx>,
    // `p` literally just means propogate, don't know what else to call it
    pub pflags: MergeFlags,
    flags: CommitNodeFlags,
    // *NOTE* We are reasoning under the assumption that committer timestamps are *non-decreasing*
    // In the absolute worst case all timestamps will be equal but a child can never be committed before parent
    // which is obviously true but maybe wrong systems times can cause issues. In bit, the committin has a check against this,
    // but unsure if git itself enforces this.
    //
    // The index which it was inserted into the queue.
    // Used to break ties when timestamps are equal
    // Larger index means it was inserted later, so we should order
    // by highest timestamp first, followed by lowest index.
    //
    // Suppose we have the following commits each with the same timestamp. The situation will not arise naturally but since
    // commits may be rewritten it is a possible state.
    //
    // A <- C
    //  ^      \
    //   \      E
    //    \    /
    //      D
    //
    // Suppose we are rooted at E which then inserts C and D into the queue. Then suppose D is yielded next which inserts A into the queue.
    // Without the ordering of this index, there is nothing to prevent A from being yielded before C which is not ideal.
    //
    // However, even with an index we don't guarantee a topological ordering. Consider the following DAG assuming all nodes have same timestamp.
    //
    //        X - Y
    //      /
    //    P
    //      \
    //        C
    //
    // Suppose we are rooted at [C, Y] (ordering of roots is significant). Firstly we would dequeue C and then enqueue P. queue is currently [Y, P].
    // Then we would dequeue Y and then enqueue X. But then P would be yielded before X which is not in topological order.
    //
    // Empirically, the lack of the index does causes major differences if we compare bit's rev-list output against git's on libgit2 for instance.
    // With the index it is very close but not identical. The only differences were cases such as the following where B and C have the same timestamp.
    // (so neither is wrong)
    // GIT   BIT
    // A     A
    // B     C
    // C     B
    // D     D
    // index: usize,
}

impl<'rcx> CommitNode<'rcx> {
    pub fn new(commit: Commit<'rcx>) -> Self {
        Self::with_pflags(commit, MergeFlags::default())
    }

    pub fn with_pflags(commit: Commit<'rcx>, pflags: MergeFlags) -> Self {
        Self { commit, pflags, flags: Default::default() }
    }
}

bitflags! {
    #[derive(Default)]
    pub struct MergeFlags: u8 {
        const PARENT1 = 1 << 0;
        const PARENT2 = 1 << 1;
        const RESULT = 1 << 2;
        const STALE = 1 << 3;
    }
}

bitflags! {
    #[derive(Default)]
    struct CommitNodeFlags: u8 {
        const YIELDED = 1 << 0;
        const ENQUEUED = 1 << 1;
        const UNINTERESTING = 1 << 2;
    }
}

impl<'rcx> Deref for CommitNode<'rcx> {
    type Target = Commit<'rcx>;

    fn deref(&self) -> &Self::Target {
        &self.commit
    }
}

// probably not an entirely sound thing to do
// but necessary for the ord impl below
impl Eq for CommitNode<'_> {
}

impl<'rcx> Commit<'rcx> {
    pub fn revwalk(self) -> BitResult<RevWalk<'rcx>> {
        RevWalk::walk_commit(self)
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn revwalk(self, revspecs: &[&Revspec]) -> BitResult<RevWalk<'rcx>> {
        RevWalk::walk_revspecs(self, revspecs)
    }
}

impl<'rcx> RevWalk<'rcx> {
    fn make(repo: BitRepo<'rcx>) -> Self {
        Self { repo, nodes: Default::default(), pqueue: Default::default(), index: 0 }
    }

    pub fn new_for_merge(a: Commit<'rcx>, b: Commit<'rcx>) -> Self {
        let mut this = Self::make(a.owner());
        this.enqueue_commit(a, MergeFlags::PARENT1);
        this.enqueue_commit(b, MergeFlags::PARENT2);
        this
    }

    pub fn new(roots: SmallVec<[Commit<'rcx>; 2]>) -> Self {
        assert!(!roots.is_empty());
        let mut this = Self::make(roots[0].owner());
        roots.into_iter().for_each(|commit| this.enqueue_commit(commit, MergeFlags::default()));
        this
    }

    fn next_index(&mut self) -> usize {
        let index = self.index;
        self.index += 1;
        index
    }

    fn dequeue_node(&mut self) -> Option<&mut CommitNode<'rcx>> {
        if !self.still_interesting() {
            return None;
        }
        // we don't actually remove the node from the nodes map as we use it check whether the commit has been seen or not
        self.pqueue.pop().and_then(move |ordering| self.nodes.get_mut(&ordering.oid))
    }

    fn enqueue_commit(&mut self, commit: Commit<'rcx>, pflags: MergeFlags) {
        let node = self.nodes.entry(commit.oid()).or_insert_with(|| CommitNode::new(commit));
        // propogate child pflags to the parent (important, even if node has already been seen)
        node.pflags.insert(pflags);

        if node.flags.intersects(CommitNodeFlags::ENQUEUED | CommitNodeFlags::YIELDED) {
            return;
        }

        node.flags.insert(CommitNodeFlags::ENQUEUED);
        let ordering = CommitNodeOrdering {
            time: node.committer.time,
            oid: node.oid(),
            index: self.next_index(),
        };
        self.pqueue.push(ordering)
    }

    pub fn walk_revspecs(repo: BitRepo<'rcx>, revspecs: &[&Revspec]) -> BitResult<Self> {
        let roots = revspecs
            .iter()
            .map(|&rev| repo.fully_resolve_rev(rev)?.peel(repo))
            .collect::<Result<SmallVec<_>, _>>()?;
        Ok(Self::new(roots))
    }

    pub fn walk_revspec(repo: BitRepo<'rcx>, rev: &Revspec) -> BitResult<Self> {
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

    fn still_interesting(&self) -> bool {
        // this function may need to be parameterised
        // it's wasteful that we might check for non-stale entries when we're not even computing a mergebase
        // in which case we know no entries are stale
        self.pqueue.iter().any(|ord| !self.nodes[&ord.oid].pflags.contains(MergeFlags::STALE))
    }
}

/// yields all commits reachable from the roots in reverse chronological order
/// parents commits are guaranteed to be yielded only after *all* their children have been yielded
impl<'rcx> FallibleIterator for RevWalk<'rcx> {
    type Error = BitGenericError;
    type Item = CommitNode<'rcx>;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        let node = match self.dequeue_node() {
            Some(node) => node,
            None => return Ok(None),
        };
        node.flags.insert(CommitNodeFlags::YIELDED);

        let parent_pflags = if node.pflags.contains(MergeFlags::PARENT1 | MergeFlags::PARENT2) {
            node.pflags.insert(MergeFlags::RESULT);
            node.pflags | MergeFlags::STALE
        } else {
            node.pflags
        };

        let node = node.clone();

        for &parent in &node.parents {
            self.enqueue_commit(self.repo.read_obj_commit(parent)?, parent_pflags);
        }

        Ok(Some(node))
    }
}
