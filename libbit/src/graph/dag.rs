#![allow(dead_code)]

mod topological;

pub use topological::TopologicalSort;

use crate::error::BitResult;
use crate::obj::Oid;
use arrayvec::ArrayVec;
use indexed_vec::{newtype_index, Idx, IndexVec};

// this needs more refactoring to be generally useful
// as we can't just enumerate all nodes in the full commit graph
pub trait Dag {
    type Node: Copy + Eq + std::hash::Hash;
    type Nodes: IntoIterator<Item = Self::Node>;
    type NodeData: DagNode<Self>;

    fn nodes(&self) -> BitResult<Self::Nodes>;
    fn node_data(&self, node: Self::Node) -> BitResult<Self::NodeData>;

    fn topological(&self) -> BitResult<TopologicalSort<'_, Self>> {
        TopologicalSort::new(self)
    }

    #[cfg(test)]
    fn reverse_topological(&self) -> BitResult<ReverseTopologicalSort<'_, Self>> {
        ReverseTopologicalSort::new(self)
    }

    #[cfg(test)]
    // iterate over all edges `u -> v` and check that `u` appears after `v` in `reverse_topological_sort`
    fn is_reverse_topological(&self, topological_sort: &[Self::Node]) -> BitResult<bool> {
        for u in self.nodes()? {
            for v in self.node_data(u)?.adjacent() {
                if topological_sort.iter().position(|&node| node == u)
                    < topological_sort.iter().position(|&node| node == v)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    #[cfg(test)]
    // iterate over all edges `u -> v` and check that `u` appears before `v` in `topological_sort`
    fn is_topological(&self, topological_sort: &[Self::Node]) -> BitResult<bool> {
        for u in self.nodes()? {
            for v in self.node_data(u)?.adjacent() {
                if topological_sort.iter().position(|&node| node == u)
                    > topological_sort.iter().position(|&node| node == v)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub trait DagNode<G: Dag + ?Sized> {
    fn adjacent(&self) -> G::Nodes;
}

newtype_index!(Node);

#[derive(Debug, Clone)]
pub struct NodeData {
    pub parents: Vec<Node>,
    pub tree: Option<Oid>,
}

impl NodeData {
    pub fn new(tree: Option<Oid>) -> Self {
        Self { tree, parents: Default::default() }
    }
}

impl<'rcx> DagNode<DagBuilder> for NodeData {
    fn adjacent(&self) -> Vec<Node> {
        self.parents.clone()
    }
}

#[derive(Default, Debug)]
pub struct DagBuilder {
    nodes: IndexVec<Node, NodeData>,
}

impl Dag for DagBuilder {
    type Node = Node;
    type NodeData = NodeData;
    type Nodes = Vec<Node>;

    fn nodes(&self) -> BitResult<Self::Nodes> {
        Ok((0..self.nodes.len()).map(Node::new).collect())
    }

    fn node_data(&self, node: Self::Node) -> BitResult<Self::NodeData> {
        Ok(self.nodes[node].clone())
    }
}

impl<'rcx> DagBuilder {
    pub fn node_data(&self, node: Node) -> &NodeData {
        &self.nodes[node]
    }

    pub fn add_parent(&mut self, child: Node, parent: Node) {
        self.nodes[child].parents.push(parent)
    }

    pub fn add_parents(&mut self, edges: impl IntoIterator<Item = (Node, Node)>) {
        for (child, parent) in edges {
            self.add_parent(child, parent);
        }
    }

    pub fn mk_node(&mut self, tree: Option<Oid>) -> Node {
        self.nodes.push(NodeData::new(tree))
    }

    /// create nodes with an empty corresponding trees
    pub fn mk_nodes<const N: usize>(&mut self) -> [Node; N] {
        (0..N).map(|_| self.mk_node(None)).collect::<ArrayVec<_, N>>().into_inner().unwrap()
    }

    pub fn mk_nodes_with_trees<const N: usize>(&mut self, trees: [Oid; N]) -> [Node; N] {
        trees
            .into_iter()
            .map(|tree| self.mk_node(Some(tree)))
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .unwrap()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reverse_topological;

#[cfg(test)]
pub use reverse_topological::ReverseTopologicalSort;
