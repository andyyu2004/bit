use crate::graph::Dag;

use super::DagBuilder;

///
///  a   -      b
///              \
///  c - d        g
///        \    /
///  e   -   f
#[test]
fn test_topological_sort() {
    let mut dag = DagBuilder::default();
    let [a, b, c, d, e, f, g] = dag.mk_nodes();

    dag.add_parents([(g, b), (g, f), (f, d), (f, e), (d, c), (b, a)]);

    let topological_sort = dag.topological().collect::<Vec<_>>();
    assert!(dag.is_topological(&topological_sort));
}

///
///  a    -     b
///              \
///  c - d        g
///        \    /
///  e   -   f
#[test]
fn test_reverse_topological_sort() {
    let mut dag = DagBuilder::default();
    let [a, b, c, d, e, f, g] = dag.mk_nodes();

    dag.add_parents([(g, b), (g, f), (f, d), (f, e), (d, c), (b, a)]);

    let topological_sort = dag.reverse_topological().collect::<Vec<_>>();
    assert!(dag.is_reverse_topological(&topological_sort));
}
