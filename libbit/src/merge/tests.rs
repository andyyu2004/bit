use crate::error::BitResult;
use crate::graph::{Dag, DagBuilder, DagNode};
use crate::obj::{BitObject, CommitMessage, Oid};
use crate::refs::SymbolicRef;
use crate::repo::BitRepo;
use crate::test_utils::generate_random_string;
use fallible_iterator::FallibleIterator;
use rustc_hash::FxHashMap;

struct CommitGraphBuilder<'rcx> {
    repo: BitRepo<'rcx>,
}

impl<'rcx> CommitGraphBuilder<'rcx> {
    pub fn new(repo: BitRepo<'rcx>) -> Self {
        Self { repo }
    }

    /// write all commits represented in `dag` to the repository
    /// returning the commits created in order of the dag nodes
    pub fn apply<G: Dag>(self, dag: &G) -> BitResult<FxHashMap<G::Node, Oid>> {
        let tree = self.repo.write_tree()?;

        // mapping from node to it's commit oid
        let mut commits = FxHashMap::<G::Node, Oid>::default();

        dag.reverse_topological()?.for_each(|node| {
            let node_data = dag.node_data(node)?;
            let parents = node_data.adjacent().into_iter().map(|parent| commits[&parent]).collect();

            let message = CommitMessage {
                subject: "generated commit".to_owned(),
                message: generate_random_string(0..100),
            };

            let commit = self.repo.mk_commit(tree, message, parents)?;
            commits.insert(node, commit);
            Ok(())
        })?;

        Ok(commits)
    }
}

/// a - b  - c - i - j
///     \       /
///      d  -  e  -  f
///       \
///        g - h
#[test]
fn test_best_common_ancestors() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let mut dag = DagBuilder::default();
        let [a, b, c, d, e, f, g, h, i, j] = dag.mk_nodes();
        dag.add_parents([
            (j, i),
            (i, e),
            (i, c),
            (c, b),
            (b, a),
            (e, d),
            (f, e),
            (h, g),
            (g, d),
            (d, b),
        ]);

        let commit_oids = CommitGraphBuilder::new(repo).apply(&dag)?;

        let a = commit_oids[&h];
        let b = commit_oids[&j];
        let merge_base = repo.merge_base(a, b)?;
        assert_eq!(merge_base.oid(), commit_oids[&d]);

        Ok(())
    })
}

#[test]
fn test_simple_merge() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let tree_a = tree! {
            foo < "foo"
            shared < "hello from a"
        };
        let tree_b = tree! {
            foo < "foo"
            shared < "hello from b"
        };
        // TODO
        Ok(())
    })
}
