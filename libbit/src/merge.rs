use crate::error::BitResult;
use crate::obj::{BitObject, Commit, Oid};
use crate::peel::Peel;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashSet;

impl<'rcx> BitRepo<'rcx> {
    pub fn merge_base(self, a: Oid, b: Oid) -> BitResult<Commit<'rcx>> {
        let a = a.peel(self)?;
        let b = b.peel(self)?;
        a.find_merge_base(b)
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
