use crate::error::BitResult;
use crate::graph::{Dag, DagBuilder, DagNode};
use crate::index::{Conflict, ConflictType};
use crate::merge::MergeKind;
use crate::obj::{BitObject, CommitMessage, Oid};
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
        dbg!(&commit_oids);
        let merge_base = repo.merge_base(a, b)?;
        assert_eq!(merge_base.oid(), commit_oids[&d]);

        Ok(())
    })
}

#[test]
fn test_simple_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: "a");
        bit_branch!(repo: "b");

        bit_checkout!(repo: "a");
        repo.checkout_tree(tree! {
            sameaddition < "foo"
            conflicted < "hello from a"
        })?;
        bit_commit_all!(repo);

        bit_checkout!(repo: "b");
        repo.checkout_tree(tree! {
            sameaddition < "foo"
            conflicted < "hello from b"
        })?;
        bit_commit_all!(repo);

        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/b"));
        bit_merge!(repo: "a");

        repo.with_index(|index| {
            assert!(index.has_conflicts());
            let conflicts = index.conflicts();
            assert_eq!(conflicts.len(), 1);
            let conflict = &conflicts[0];
            assert_eq!(
                conflict,
                &Conflict { path: p!("conflicted"), conflict_type: ConflictType::BothAdded }
            );
            Ok(())
        })
    })
}

#[test]
fn test_merge_conflict_types() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: "alternative");

        // on `master`
        modify!(repo: "bar");
        modify!(repo: "dir/baz");
        rm!(repo: "foo");
        bit_commit_all!(repo);

        // on `alternative`
        bit_checkout!(repo: "alternative");
        modify!(repo: "foo");
        modify!(repo: "dir/baz");
        rm!(repo: "bar");
        bit_commit_all!(repo);

        bit_merge!(repo: "master");

        repo.with_index(|index| {
            assert!(index.has_conflicts());
            let conflicts = index.conflicts();
            assert_eq!(
                conflicts,
                vec![
                    Conflict { path: p!("bar"), conflict_type: ConflictType::DeleteModify },
                    Conflict { path: p!("dir/baz"), conflict_type: ConflictType::BothModified },
                    Conflict { path: p!("foo"), conflict_type: ConflictType::ModifyDelete }
                ]
            );
            Ok(())
        })
    })
}

#[test]
fn test_null_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: -b "b");
        modify!(repo: "foo");
        bit_commit_all!(repo);
        let merge_kind = bit_merge!(repo: "master");
        assert_eq!(merge_kind, MergeKind::Null);
        Ok(())
    })
}

#[test_env_log::test]
fn test_ff_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: -b "b");
        modify!(repo: "foo");
        bit_commit_all!(repo);
        bit_checkout!(repo: "master");
        let merge_kind = bit_merge!(repo: "b");
        assert_eq!(merge_kind, MergeKind::FastForward);
        Ok(())
    })
}
