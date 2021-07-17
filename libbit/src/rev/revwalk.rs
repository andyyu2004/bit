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
use std::iter::FromIterator;

pub struct RevWalk<'rcx> {
    repo: BitRepo<'rcx>,
    flags: FxHashMap<Oid, CommitNodeFlags>,
    queue: BinaryHeap<Commit<'rcx>>,
}

bitflags! {
    struct CommitNodeFlags: u8 {
        const SEEN = 1;
    }
}

impl<'rcx> PartialOrd for Commit<'rcx> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.author.time.cmp(&other.author.time) {
            Ordering::Less => Some(Ordering::Less),
            Ordering::Equal => None,
            Ordering::Greater => Some(Ordering::Greater),
        }
    }
}

// probably not an entirely sound thing to do
impl Eq for Commit<'_> {
}

impl<'rcx> Ord for Commit<'rcx> {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.partial_cmp(other) {
            Some(ordering) => ordering,
            None => todo!(),
        }
    }
}

impl<'rcx> RevWalk<'rcx> {
    pub fn new(roots: SmallVec<[Commit<'rcx>; 2]>) -> Self {
        debug_assert!(!roots.is_empty());
        Self {
            repo: roots[0].owner(),
            queue: BinaryHeap::from_iter(roots),
            flags: Default::default(),
        }
    }

    pub fn enqueue_commit(&mut self, commit: Commit<'rcx>) {
        if self.flags[&commit.oid()].contains(CommitNodeFlags::SEEN) {
            return;
        }
        self.queue.push(commit)
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
        let commit = match self.queue.pop() {
            Some(commit) => commit,
            None => return Ok(None),
        };

        if let Some(parent) = commit.parent {
            self.enqueue_commit(self.repo.read_obj(parent)?.into_commit());
        }

        Ok(Some(commit))
    }
}
