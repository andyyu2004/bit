use crate::error::BitResult;
use crate::index::{BitIndexEntry, MergeStage};
use crate::iter::{BitEntry, BitTreeIterator};
use crate::obj::{BitObject, Commit, MutableBlob, Oid, TreeEntry};
use crate::peel::Peel;
use crate::refs::BitRef;
use crate::repo::BitRepo;
use crate::rev::Revspec;
use crate::xdiff;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashSet;
use std::io::Write;

pub type ConflictStyle = diffy::ConflictStyle;

impl<'rcx> BitRepo<'rcx> {
    pub fn merge_base(self, a: Oid, b: Oid) -> BitResult<Commit<'rcx>> {
        let commit_a = a.peel(self)?;
        let commit_b = b.peel(self)?;
        commit_a.find_merge_base(commit_b)
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

impl<'rcx> Commit<'rcx> {
    /// Returns lowest common ancestor found.
    /// Only returns a single solution even when there may be multiple valid/optimal solutions.
    // TODO
    // I'm pretty sure this function will not work in all cases (i.e. return a non-optimal solution)
    // Not sure if those cases will come up realistically though, to investigate
    pub fn find_merge_base(self, other: Commit<'rcx>) -> BitResult<Commit<'rcx>> {
        debug_assert_eq!(self.owner(), other.owner());

        let mut iter_self = self.revwalk()?;
        let mut iter_other = other.revwalk()?;

        let mut xs = FxHashSet::default();
        let mut ys = FxHashSet::default();

        macro_rules! handle {
            ($xs:expr, $ys:expr, $x:expr) => {{
                if $ys.contains(&$x.oid()) {
                    return Ok($x);
                }
                $xs.insert($x.oid());
            }};
        }

        macro_rules! handle_x {
            ($x:expr) => {
                handle!(xs, ys, $x)
            };
        }

        macro_rules! handle_y {
            ($y:expr) => {
                handle!(ys, xs, $y)
            };
        }

        // keep track of nodes for each iterator and return when a "self node" is contained in "other nodes" or vice versa
        loop {
            match (iter_self.next()?, iter_other.next()?) {
                (Some(x), Some(y)) => {
                    handle_x!(x);
                    handle_y!(y);
                }
                (Some(x), _) => handle_x!(x),
                (_, Some(y)) => handle_y!(y),
                (None, None) => panic!("no merge base found, two completely disjoint histories?"),
            }
        }
    }
}

#[cfg(test)]
mod tests;
