use crate::error::BitResult;
use crate::graph::{Dag, DagBuilder, DagNode};
use crate::obj::{BitObject, CommitMessage, Oid};
use crate::repo::BitRepo;
use crate::test_utils::generate_random_string;
use indexed_vec::IndexVec;

struct CommitGraphBuilder<'rcx> {
    repo: BitRepo<'rcx>,
}

impl<'rcx> CommitGraphBuilder<'rcx> {
    pub fn new(repo: BitRepo<'rcx>) -> Self {
        Self { repo }
    }

    /// write all commits represented in `dag` to the repository
    /// returning the commits created in order of the dag nodes
    pub fn apply<G: Dag>(self, dag: &G) -> BitResult<Vec<Oid>> {
        let tree = self.repo.write_tree()?;

        // mapping from node to it's commit oid
        let mut commits: IndexVec<G::Node, Option<Oid>> =
            IndexVec::from_elem_n(None, dag.nodes().len());

        for node in dag.reverse_topological() {
            let node_data = dag.node_data(node);
            let parents = node_data
                .adjacent()
                .iter()
                .map(|&parent| {
                    commits[parent].expect("parent commit should be materialized already")
                })
                .collect();

            let message = CommitMessage {
                subject: "generated commit".to_owned(),
                message: generate_random_string(0..100),
            };

            let commit = self.repo.mk_commit(tree, message, parents)?;
            commits[node] = Some(commit);
        }
        Ok(commits.into_iter().map(|opt| opt.unwrap()).collect())
    }
}

/// 0 - 1  - 2 - 8 - 9
///     \       /
///      3  -  4  -  5
///       \
///        6 - 7
#[test]
fn multiple_best_common_ancestors() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let mut dag = DagBuilder::default();
        let [c0, c1, c2, c3, c4, c5, c6, c7, c8, c9] = dag.mk_nodes::<10>();
        dag.add_parents([
            (c9, c8),
            (c8, c4),
            (c8, c2),
            (c2, c1),
            (c1, c0),
            (c4, c3),
            (c5, c4),
            (c7, c6),
            (c6, c3),
            (c3, c1),
        ]);

        let commit_oids = CommitGraphBuilder::new(repo).apply(&dag)?;

        let a = commit_oids[7];
        let b = commit_oids[9];
        let bca = repo.merge_base(a, b)?;
        // `bca(7, 9) = 3`
        assert_eq!(bca.oid(), commit_oids[3]);

        Ok(())
    })
}
