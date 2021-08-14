use crate::error::BitGenericError;

use super::*;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

// non-iterative implementation
// this is only used in test code for now so doesn't matter too much
pub struct ReverseTopologicalSort<'a, G: Dag + ?Sized> {
    dag: &'a G,
    visited: FxHashSet<G::Node>,
    solution: VecDeque<G::Node>,
}

impl<'a, G: Dag + ?Sized> ReverseTopologicalSort<'a, G> {
    pub fn new(dag: &'a G) -> BitResult<Self> {
        let mut this = Self { dag, visited: Default::default(), solution: Default::default() };
        this.solve()?;
        Ok(this)
    }

    // populate `self.solution`
    fn solve(&mut self) -> BitResult<()> {
        for node in self.dag.nodes()? {
            self.solve_node(node)?;
        }
        Ok(())
    }

    fn solve_node(&mut self, node: G::Node) -> BitResult<()> {
        if self.visited.contains(&node) {
            return Ok(());
        }
        self.visited.insert(node);

        for v in self.dag.node_data(node)?.adjacent() {
            self.solve_node(v)?;
        }

        self.solution.push_back(node);
        Ok(())
    }
}

impl<'a, G: Dag> FallibleIterator for ReverseTopologicalSort<'a, G> {
    type Error = BitGenericError;
    type Item = G::Node;

    fn next(&mut self) -> BitResult<Option<Self::Item>> {
        Ok(self.solution.pop_front())
    }
}
