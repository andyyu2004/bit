use super::{Dag, DagNode};
use indexed_vec::IndexVec;
use std::collections::VecDeque;

// implement Kahn's algorithm
pub struct TopologicalSort<'a, G: Dag + ?Sized> {
    dag: &'a G,
    indegrees: IndexVec<G::Node, u32>,
    queue: VecDeque<G::Node>,
}

impl<'a, G: Dag + ?Sized> TopologicalSort<'a, G> {
    pub fn new(dag: &'a G) -> Self {
        let mut indegrees = IndexVec::from_elem_n(0, dag.nodes().len());

        for node_data in dag.nodes() {
            for &parent in node_data.adjacent() {
                indegrees[parent] += 1;
            }
        }

        // start queue with all nodes that have no indegree
        let queue = indegrees
            .iter_enumerated()
            .filter(|(_, &indegree)| indegree == 0)
            .map(|(node, _)| node)
            .collect();

        Self { dag, indegrees, queue }
    }
}

impl<'a, G: Dag> Iterator for TopologicalSort<'a, G> {
    type Item = G::Node;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.queue.pop_front()?;
        for &parent in self.dag.node_data(node).adjacent() {
            let indegree = &mut self.indegrees[parent];
            *indegree -= 1;
            if *indegree == 0 {
                self.queue.push_back(parent)
            }
        }
        Some(node)
    }
}
