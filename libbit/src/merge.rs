use crate::error::BitResult;
use crate::index::{BitIndexEntry, MergeStage};
use crate::iter::BitTreeIterator;
use crate::obj::{BitObject, Commit, MutableBlob, Oid, TreeEntry};
use crate::peel::Peel;
use crate::repo::BitRepo;
use crate::xdiff;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashSet;

pub type ConflictStyle = diffy::ConflictStyle;

impl<'rcx> BitRepo<'rcx> {
    pub fn merge_base(self, a: Oid, b: Oid) -> BitResult<Commit<'rcx>> {
        let a = a.peel(self)?;
        let b = b.peel(self)?;
        a.find_merge_base(b)
    }

    pub fn merge(self, their_head: Oid) -> BitResult<()> {
        MergeCtxt { repo: self }.merge(their_head)
    }
}

#[derive(Debug)]
struct MergeCtxt<'rcx> {
    repo: BitRepo<'rcx>,
}

impl<'rcx> MergeCtxt<'rcx> {
    fn new(repo: BitRepo<'rcx>) -> Self {
        Self { repo }
    }

    pub fn merge(&mut self, their_head: Oid) -> BitResult<()> {
        let repo = self.repo;
        let our_head = repo.fully_resolve_head()?;
        let our_head_commit = our_head.peel(repo)?;
        let their_head_commit = their_head.peel(repo)?;
        let merge_base = our_head_commit.find_merge_base(their_head_commit)?;

        if merge_base.oid() == their_head {
            bail!("Already up to date")
        }

        if merge_base.oid() == our_head {
            todo!("ff merge?")
        }

        self.merge_from_iterators(
            repo.tree_iter(merge_base.oid()).ignore_trees(),
            repo.tree_iter(our_head).ignore_trees(),
            repo.tree_iter(their_head).ignore_trees(),
        )?;
        todo!()
    }

    pub fn merge_from_iterators(
        &mut self,
        base_iter: impl BitTreeIterator,
        a_iter: impl BitTreeIterator,
        b_iter: impl BitTreeIterator,
    ) -> BitResult<()> {
        let repo = self.repo;
        let walk = repo.walk_iterators([Box::new(base_iter), Box::new(a_iter), Box::new(b_iter)]);
        walk.for_each(|[base, a, b]| self.merge_entries(base, a, b))
    }

    fn merge_entries(
        &mut self,
        base: Option<BitIndexEntry>,
        a: Option<BitIndexEntry>,
        b: Option<BitIndexEntry>,
    ) -> BitResult<()> {
        let repo = self.repo;
        repo.with_index_mut(|index| {
            let mut three_way_merge =
                |base: Option<BitIndexEntry>, y: BitIndexEntry, z: BitIndexEntry| {
                    let base_str = match base {
                        Some(b) => b.read_to_string(repo)?,
                        None => String::new(),
                    };

                    if y.mode != z.mode {
                        todo!("mode conflict");
                    }

                    match xdiff::merge(
                        repo.config().conflictStyle()?,
                        &base_str,
                        &y.read_to_string(repo)?,
                        &z.read_to_string(repo)?,
                    ) {
                        Ok(merged) => {
                            let oid = repo.write_obj(&MutableBlob::new(merged.into_bytes()))?;
                            index.add_entry(TreeEntry { oid, path: y.path, mode: y.mode }.into())
                        }
                        Err(conflicted) => {
                            let _oid = repo.write_obj(&MutableBlob::new(conflicted.into_bytes()))?;
                            todo!()
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
                    index.add_conflicted_entry(b, MergeStage::One)?;
                    index.add_conflicted_entry(y, MergeStage::Two)
                }
                (Some(b), None, Some(z)) => {
                    index.add_conflicted_entry(b, MergeStage::One)?;
                    index.add_conflicted_entry(z, MergeStage::Three)
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
