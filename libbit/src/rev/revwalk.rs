use super::Revspec;
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

#[derive(Debug, Clone)]
pub struct RevWalk<'rcx> {
    repo: BitRepo<'rcx>,
    // map of commit oid to their flags
    // I suppose this field name should be doubly plural
    flags: FxHashMap<Oid, CommitNodeFlags>,
    pqueue: BinaryHeap<CommitNode<'rcx>>,
    index: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct CommitNode<'rcx> {
    commit: &'rcx Commit<'rcx>,
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
// but necessary for the ord impl below
impl Eq for CommitNode<'_> {
}

impl<'rcx> Ord for CommitNode<'rcx> {
    // we want this cmp to suit a maxheap
    // so we want the most recent (largest timestamp) commit to be >= and the smallest index to be >=
    fn cmp(&self, other: &Self) -> Ordering {
        self.committer
            .time
            .cmp(&other.committer.time)
            .then_with(|| other.index.cmp(&self.index))
            .then_with(|| bug!())
    }
}

impl<'rcx> Commit<'rcx> {
    pub fn revwalk(&'rcx self) -> BitResult<RevWalk<'rcx>> {
        RevWalk::walk_commit(self)
    }
}

impl<'rcx> BitRepo<'rcx> {
    pub fn revwalk(self, revspecs: &[&Revspec]) -> BitResult<RevWalk<'rcx>> {
        RevWalk::walk_revspecs(self, revspecs)
    }
}

impl<'rcx> RevWalk<'rcx> {
    pub fn new(roots: SmallVec<[&'rcx Commit<'rcx>; 2]>) -> Self {
        debug_assert!(!roots.is_empty());
        let mut this = Self {
            repo: roots[0].owner(),
            flags: Default::default(),
            pqueue: Default::default(),
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

    fn mk_node(&mut self, commit: &'rcx Commit<'rcx>) -> CommitNode<'rcx> {
        CommitNode { commit, index: self.next_index() }
    }

    pub fn enqueue_commit(&mut self, commit: &'rcx Commit<'rcx>) {
        let flags = self.flags.entry(commit.oid()).or_default();
        if flags.intersects(CommitNodeFlags::ENQUEUED | CommitNodeFlags::YIELDED) {
            return;
        }
        flags.insert(CommitNodeFlags::ENQUEUED);
        let node = self.mk_node(commit);
        self.pqueue.push(node)
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

    pub fn walk_commit(root: &'rcx Commit<'rcx>) -> BitResult<Self> {
        Self::walk_commits(std::iter::once(root))
    }

    pub fn walk_commits(roots: impl IntoIterator<Item = &'rcx Commit<'rcx>>) -> BitResult<Self> {
        let roots = roots.into_iter().collect::<SmallVec<_>>();
        ensure!(!roots.is_empty());
        Ok(Self::new(roots))
    }
}

/// yields all commits reachable from the roots in reverse chronological order
/// parents commits are guaranteed to be yielded only after *all* their children have been yielded
impl<'rcx> FallibleIterator for RevWalk<'rcx> {
    type Error = BitGenericError;
    type Item = &'rcx Commit<'rcx>;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        let node = match self.pqueue.pop() {
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
