use super::*;
use bit_set::BitSet;
use std::collections::VecDeque;

// non-iterative implementation
// this is only used in test code for now so doesn't matter too much
pub struct ReverseTopologicalSort<'a, G: Dag + ?Sized> {
    dag: &'a G,
    visited: BitSet<usize>,
    solution: VecDeque<G::Node>,
}

impl<'a, G: Dag + ?Sized> ReverseTopologicalSort<'a, G> {
    pub fn new(dag: &'a G) -> Self {
        let mut this = Self { dag, visited: Default::default(), solution: Default::default() };
        this.solve();
        this
    }

    // populate `self.solution`
    fn solve(&mut self) {
        for u in 0..self.dag.nodes().len() {
            self.solve_node(G::Node::new(u))
        }
    }

    fn solve_node(&mut self, node: G::Node) {
        if self.visited.contains(node.index()) {
            return;
        }
        self.visited.insert(node.index());
        for &v in self.dag.node_data(node).adjacent() {
            self.solve_node(v);
        }
        self.solution.push_back(node)
    }
}

impl<'a, G: Dag> Iterator for ReverseTopologicalSort<'a, G> {
    type Item = G::Node;

    fn next(&mut self) -> Option<Self::Item> {
        self.solution.pop_front()
    }
}
