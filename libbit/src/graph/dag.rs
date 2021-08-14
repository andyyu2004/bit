use arrayvec::ArrayVec;
use indexed_vec::{newtype_index, Idx, IndexVec};
pub use topological::TopologicalSort;

#[cfg(test)]
pub use reverse_topological::ReverseTopologicalSort;
#[cfg(test)]
mod reverse_topological;

mod topological;

pub trait Dag {
    type Node: Idx;
    type NodeData: DagNode<Self>;
    fn nodes(&self) -> &IndexVec<Self::Node, Self::NodeData>;
    fn node_data(&self, node: Self::Node) -> &Self::NodeData;

    fn topological(&self) -> TopologicalSort<'_, Self> {
        TopologicalSort::new(self)
    }

    #[cfg(test)]
    fn reverse_topological(&self) -> ReverseTopologicalSort<'_, Self> {
        ReverseTopologicalSort::new(self)
    }

    #[cfg(test)]
    // iterate over all edges `u -> v` and check that `u` appears after `v` in `reverse_topological_sort`
    fn is_reverse_topological(&self, topological_sort: &[Self::Node]) -> bool {
        for (u, node_data) in self.nodes().iter_enumerated() {
            for &v in node_data.adjacent() {
                if topological_sort.iter().position(|&node| node == u)
                    < topological_sort.iter().position(|&node| node == v)
                {
                    return false;
                }
            }
        }
        true
    }

    #[cfg(test)]
    // iterate over all edges `u -> v` and check that `u` appears before `v` in `topological_sort`
    fn is_topological(&self, topological_sort: &[Self::Node]) -> bool {
        for (u, node_data) in self.nodes().iter_enumerated() {
            for &v in node_data.adjacent() {
                if topological_sort.iter().position(|&node| node == u)
                    > topological_sort.iter().position(|&node| node == v)
                {
                    return false;
                }
            }
        }
        true
    }
}

pub trait DagNode<G: Dag + ?Sized> {
    fn adjacent(&self) -> &[G::Node];
}

newtype_index!(Node);

#[derive(Debug, Default, Clone)]
pub struct NodeData {
    parents: Vec<Node>,
}

impl<'rcx> DagNode<DagBuilder> for NodeData {
    fn adjacent(&self) -> &[Node] {
        &self.parents
    }
}

#[derive(Default, Debug)]
pub struct DagBuilder {
    nodes: IndexVec<Node, NodeData>,
}

impl Dag for DagBuilder {
    type Node = Node;
    type NodeData = NodeData;

    fn nodes(&self) -> &IndexVec<Self::Node, Self::NodeData> {
        &self.nodes
    }

    fn node_data(&self, node: Self::Node) -> &Self::NodeData {
        &self.nodes[node]
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

    pub fn mk_node(&mut self) -> Node {
        self.nodes.push(NodeData::default())
    }

    pub fn mk_nodes<const N: usize>(&mut self) -> [Node; N] {
        (0..N).map(|_| self.mk_node()).collect::<ArrayVec<_, N>>().into_inner().unwrap()
    }
}

#[cfg(test)]
mod tests;
