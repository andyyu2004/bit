use crate::error::{BitErrorExt, BitResult};
use crate::graph::{Dag, DagBuilder, DagNode, Node};
use crate::index::{Conflict, ConflictType};
use crate::merge::MergeResults;
use crate::obj::{BitObject, CommitMessage, Oid, Treeish};
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
    pub fn apply(self, dag: &DagBuilder) -> BitResult<FxHashMap<Node, Oid>> {
        // mapping from node to it's commit oid
        let mut commits = FxHashMap::<Node, Oid>::default();

        dag.reverse_topological()?.for_each(|node| {
            let node_data = dag.node_data(node);
            let parents = node_data.adjacent().into_iter().map(|parent| commits[&parent]).collect();

            let message = CommitMessage {
                subject: "generated commit".to_owned(),
                message: generate_random_string(0..100),
            };

            let tree = match node_data.tree {
                Some(tree) => tree,
                None => self.repo.write_tree()?,
            };
            let commit = self.repo.write_commit(tree, parents, message)?;
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
        let merge_base = repo.merge_base(a, b)?.unwrap();
        assert_eq!(merge_base.oid(), commit_oids[&d]);

        Ok(())
    })
}

// a - c
//   X
// b - d
#[test]
fn test_criss_cross_merge_base() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let mut dag = DagBuilder::default();
        let [a, b, c, d] = dag.mk_nodes();
        dag.add_parents([(c, a), (c, b), (d, a), (d, b)]);

        let commits = CommitGraphBuilder::new(repo).apply(&dag)?;

        let merge_bases = repo.merge_bases(commits[&c], commits[&d])?;
        assert_eq!(merge_bases.len(), 2);
        assert_eq!(merge_bases[0].oid(), commits[&a]);
        assert_eq!(merge_bases[1].oid(), commits[&b]);

        Ok(())
    })
}

#[test_env_log::test]
fn test_trivial_criss_cross_merge() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let tree = tree! {
            foo < "foo contents"
        };

        let mut dag = DagBuilder::default();
        let [a, b, c, d] = dag.mk_nodes_with_trees([tree, tree, tree, tree]);
        dag.add_parents([(c, a), (c, b), (d, a), (d, b)]);

        let commits = CommitGraphBuilder::new(repo).apply(&dag)?;

        bit_reset!(repo: --hard rev!(commits[&c]));
        assert_eq!(cat!(repo: "foo"), "foo contents");
        bit_branch!(repo: "d" @ rev!(commits[&d]));
        bit_merge!(repo: "d")?;
        assert!(bit_status!(repo).is_empty());

        Ok(())
    })
}

//    a  -  c
//  /
// O    X
//  \
//   b  -  d
// TODO test behaviour when a and b have conflicts, probably introduce a parent commit for them too
// #[test_env_log::test]
fn test_nontrivial_criss_cross_merge() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let tree_o = tree! {
            foo < "foo"
        };
        let tree_a = tree! {
            foo < "foo\nafter"
        };

        let tree_b = tree! {
            foo < "before\nfoo"
        };

        // a `merge` b
        // before
        // foo
        // after

        let tree_c = tree! {
            foo < "removed before\nfoo\nafter"
        };

        let tree_d = tree! {
            foo < "before\nfoo\nremoved after"
        };

        let mut dag = DagBuilder::default();
        let [o, a, b, c, d] = dag.mk_nodes_with_trees([tree_o, tree_a, tree_b, tree_c, tree_d]);
        dag.add_parents([(a, o), (b, o), (c, a), (c, b), (d, a), (d, b)]);

        let commits = CommitGraphBuilder::new(repo).apply(&dag)?;

        bit_reset!(repo: --hard rev!(commits[&c]));
        bit_branch!(repo: "d" @ rev!(commits[&d]));

        bit_merge!(repo: "d")?;

        Ok(())
    })
}

#[test]
fn test_simple_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: "a");
        bit_branch!(repo: "b");

        bit_checkout!(repo: "a")?;
        repo.checkout_tree(tree! {
            sameaddition < "foo"
            conflicted < "hello from a"
        })?;
        bit_commit_all!(repo);

        bit_checkout!(repo: "b")?;
        repo.checkout_tree(tree! {
            sameaddition < "foo"
            conflicted < "hello from b"
        })?;
        bit_commit_all!(repo);

        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/b"));

        let conflicts = bit_merge!(repo: "a")?.into_conflicts();
        assert_eq!(conflicts.len(), 1);
        let conflict = &conflicts[0];
        assert_eq!(
            conflict,
            &Conflict { path: p!("conflicted"), conflict_type: ConflictType::BothAdded }
        );
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_conflict_types() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_branch!(repo: "alternative");

        // on `master`
        modify!(repo: "bar");
        modify!(repo: "dir/baz");
        rm!(repo: "foo");
        bit_commit_all!(repo);

        // on `alternative`
        bit_checkout!(repo: "alternative")?;
        modify!(repo: "foo");
        modify!(repo: "dir/baz");
        rm!(repo: "bar");
        bit_commit_all!(repo);

        let conflicts = bit_merge!(repo: "master")?.into_conflicts();
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
}

#[test]
fn test_null_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_checkout!(repo: -b "b")?;
        modify!(repo: "foo");
        bit_commit_all!(repo);
        let merge_results = bit_merge!(repo: "master")?;
        assert_eq!(merge_results, MergeResults::Null);
        Ok(())
    })
}

#[test]
fn test_fast_forward_merge() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        bit_checkout!(repo: -b "b")?;
        modify!(repo: "foo");
        bit_commit_all!(repo);
        bit_checkout!(repo: "master")?;
        let merge_results = bit_merge!(repo: "b")?;
        assert!(matches!(merge_results, MergeResults::FastForward { .. }));

        // HEAD should not have moved
        assert_eq!(repo.read_head()?, symbolic_ref!("refs/heads/master"));
        // But `master` itself should now point to the same commit as `b`
        assert_eq!(repo.fully_resolve_ref("master")?, repo.fully_resolve_ref("b")?);
        Ok(())
    })
}

impl<'rcx> BitRepo<'rcx> {
    fn three_way_merge(
        self,
        ours: impl Treeish<'rcx>,
        theirs: impl Treeish<'rcx>,
    ) -> BitResult<MergeResults> {
        self.setup_three_way_merge(ours, theirs)?;
        bit_merge!(self: "theirs")
    }

    /// Setup a repository to have the following structure
    ///        ours
    ///      /  ^
    /// base   HEAD
    ///      \
    ///       theirs
    /// Where `base` is the old HEAD
    /// Then merge theirs into HEAD
    fn setup_three_way_merge(
        self,
        ours: impl Treeish<'rcx>,
        theirs: impl Treeish<'rcx>,
    ) -> BitResult<()> {
        // empty commit just to allow this to work starting from an empty repo
        bit_commit!(self: --allow-empty);
        bit_branch!(self: "base");

        bit_checkout!(self: -b "theirs")?;
        self.checkout_tree(theirs)?;
        bit_commit!(self: --allow-empty);

        bit_checkout!(self: "base")?;
        bit_checkout!(self: -b "ours")?;
        self.checkout_tree(ours)?;
        bit_commit!(self: --allow-empty);
        Ok(())
    }

    /// Same as `three_way_merge` except the base is reset to the provided base commit
    fn three_way_merge_with_base(
        self,
        base: Oid,
        ours: impl Treeish<'rcx>,
        theirs: impl Treeish<'rcx>,
    ) -> BitResult<MergeResults> {
        bit_reset!(self: --hard &rev!(base));
        self.three_way_merge(ours, theirs)
    }
}

#[test]
fn test_merge_base_to_ours_only_with_dir() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let ours = commit! {
            dir {
                foo < "foo"
            }
        };
        let theirs = commit! {};
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir/foo"), "foo");
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_base_to_theirs_only_with_dir() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            dir {
                foo < "foo"
            }
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir/foo"), "foo");
        Ok(())
    })
}

/// Test case where the file is present only in our head, not in base or other
/// This means we added the file and it should be kept
#[test]
fn test_merge_our_head_only() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let base = commit! {};
        let ours = commit! {
            foo < "hello"
        };
        let theirs = commit! {};
        repo.three_way_merge_with_base(base, ours, theirs)?;
        assert_eq!(cat!(repo: "foo"), "hello");
        Ok(())
    })
}

/// Test case where the file is present only in base
/// Both new heads deleted the file so it should be deleted
#[test]
fn test_merge_base_only() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {};
        let theirs = commit! {};
        repo.three_way_merge(ours, theirs)?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

/// Test case where the file is present only in their head
#[test]
fn test_merge_theirs_only() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let base = commit! {};
        let ours = commit! {};
        let theirs = commit! {
            foo < "theirs"
        };
        repo.three_way_merge_with_base(base, ours, theirs)?;
        assert_eq!(cat!(repo: "foo"), "theirs");
        Ok(())
    })
}

#[test]
fn test_merge_deleted_in_ours_and_unchanged_in_theirs() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            foo < "default foo contents"
        };
        repo.three_way_merge(ours, theirs)?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

#[test]
fn test_merge_unchanged_in_ours_and_deleted_in_theirs() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "default foo contents"
        };
        let theirs = commit! {};
        repo.three_way_merge(ours, theirs)?;
        assert!(!exists!(repo: "foo"));
        Ok(())
    })
}

#[test]
fn test_merge_deleted_in_ours_and_modified_in_theirs() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            foo < "modified"
        };
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        assert_eq!(conflicts[0], Conflict::new_with_type(p!("foo"), ConflictType::DeleteModify));
        Ok(())
    })
}

#[test]
fn test_merge_modified_in_ours_and_deleted_in_theirs() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "modified"
        };
        let theirs = commit! {};
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        assert_eq!(conflicts[0], Conflict::new_with_type(p!("foo"), ConflictType::ModifyDelete));
        Ok(())
    })
}

#[test]
fn test_merge_with_unclean_worktree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "default foo contents"
        };
        let theirs = commit! {};
        repo.setup_three_way_merge(ours, theirs)?;
        modify!(repo: "foo" < "modified");
        Ok(())
    })
}

#[test]
fn test_merge_tree_into_unmodified_file() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "default foo contents"
        };
        let theirs = commit! {
            foo {
                bar < "bar contents"
            }
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_tree_into_modified_file() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "modified foo contents"
        };
        let theirs = commit! {
            foo {
                bar < "bar contents"
            }
        };
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("foo"), ConflictType::ModifyDelete)
        );
        assert_eq!(cat!(repo: "foo~HEAD"), "modified foo contents");
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_modified_blob_into_unmodified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "default foo contents"
        };
        let theirs = commit! {
            foo < "modified foo contents"
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo"), "modified foo contents");
        Ok(())
    })
}

#[test]
fn test_merge_unmodified_blob_into_modified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "modified foo contents"
        };
        let theirs = commit! {
            foo < "default foo contents"
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo"), "modified foo contents");
        Ok(())
    })
}

#[test]
fn test_merge_unmodified_blob_into_blob_into_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "bar contents"
            }
        };
        let theirs = commit! {
            foo < "default foo contents"
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_blob_into_tree_into_unmodified_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo < "default foo contents"
        };
        let theirs = commit! {
            foo {
                bar < "bar contents"
            }
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_modified_file_into_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "bar contents"
            }
        };
        let theirs = commit! {
            foo < "modified foo contents"
        };
        touch!(repo: "foo~theirs");
        touch!(repo: "foo~theirs_0");
        touch!(repo: "foo~theirs_2");
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("foo"), ConflictType::DeleteModify)
        );
        assert_eq!(cat!(repo: "foo~theirs_1"), "modified foo contents");
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_deleted_tree_into_modified_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir {
                bar < "modified bar contents"
            }
        };
        let theirs = commit! {};
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir/bar"), ConflictType::ModifyDelete)
        );
        assert_eq!(cat!(repo: "dir/bar"), "modified bar contents");
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_modified_tree_into_deleted_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            dir {
                bar < "modified bar contents"
            }
        };
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir/bar"), ConflictType::DeleteModify)
        );
        assert_eq!(cat!(repo: "dir/bar"), "modified bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_blob_to_tree_into_deleted_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            foo {
                bar < "bar contents"
            }
        };
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_deleted_blob_into_blob_to_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "bar contents"
            }
        };
        let theirs = commit! {};
        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_tree_to_blob_into_modified_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir {
                bar < "modified bar contents"
            }
        };
        let theirs = commit! {
            dir < "dir is now a file"
        };
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir"), ConflictType::AddedByThem)
        );
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir/bar"), ConflictType::ModifyDelete)
        );
        assert_eq!(cat!(repo: "dir~theirs"), "dir is now a file");
        assert_eq!(cat!(repo: "dir/bar"), "modified bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_modified_tree_into_tree_to_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir < "dir is now a file"
        };
        let theirs = commit! {
            dir {
                bar < "modified bar contents"
            }
        };
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir"), ConflictType::AddedByUs)
        );
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("dir/bar"), ConflictType::DeleteModify)
        );
        assert_eq!(cat!(repo: "dir~HEAD"), "dir is now a file");
        assert_eq!(cat!(repo: "dir/bar"), "modified bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_unmodified_tree_into_deleted_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            dir {
                bar < "default bar contents"
            }
        };
        repo.three_way_merge(ours, theirs)?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

#[test]
fn test_merge_unmodified_tree_into_nested_deleted_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let base = commit! {
            dir {
                nested {
                    bar < "default bar contents"
                }
                bar
            }
        };
        let ours = commit! {};
        let theirs = base;
        repo.three_way_merge_with_base(base, ours, theirs)?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_deleted_tree_into_unmodified_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir {
                bar < "default bar contents"
            }
        };
        let theirs = commit! {};
        repo.three_way_merge(ours, theirs)?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

#[test]
fn test_merge_nested_deleted_tree_into_unmodified_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let base = commit! {
            dir {
                nested {
                    bar < "default bar contents"
                }
                bar
            }
        };
        let ours = base;
        let theirs = commit! {};
        repo.three_way_merge_with_base(base, ours, theirs)?;
        assert!(!exists!(repo: "dir"));
        Ok(())
    })
}

#[test]
fn test_merge_created_tree_into_created_blob() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let ours = commit! {
            foo < "some foo contents"
        };
        let theirs = commit! {
            foo {
                bar < "some bar contents"
            }
        };

        touch!(repo: "foo~HEAD" < "original foo~HEAD");
        touch!(repo: "foo~HEAD_0" < "original foo~HEAD_0");
        touch!(repo: "foo~HEAD_1");
        touch!(repo: "foo~HEAD_2");
        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("foo"), ConflictType::AddedByUs)
        );
        assert_eq!(cat!(repo: "foo~HEAD_3"), "some foo contents");
        assert_eq!(cat!(repo: "foo/bar"), "some bar contents");
        Ok(())
    })
}

#[test]
fn test_merge_created_blob_into_created_tree() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "some bar contents"
            }
        };
        let theirs = commit! {
            foo < "some foo contents"
        };

        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("foo"), ConflictType::AddedByThem)
        );
        assert_eq!(cat!(repo: "foo~theirs"), "some foo contents");
        assert_eq!(cat!(repo: "foo/bar"), "some bar contents");
        Ok(())
    })
}

#[test_env_log::test]
fn test_merge_tree_to_blob_into_unmodified_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir {
                bar < "default bar contents"
            }
        };

        let theirs = commit! {
            dir < "typechange blob->tree"
        };

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir"), "typechange blob->tree");
        Ok(())
    })
}

#[test]
fn test_merge_unmodified_tree_into_tree_to_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir < "typechange blob->tree"
        };
        let theirs = commit! {
            dir {
                bar < "default bar contents"
            }
        };

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir"), "typechange blob->tree");
        Ok(())
    })
}

#[test]
fn test_merge_tree_to_blob_into_deleted_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {};
        let theirs = commit! {
            dir < "dir contents"
        };

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir"), "dir contents");
        Ok(())
    })
}

#[test]
fn test_merge_deleted_tree_into_tree_to_blob() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir < "dir contents"
        };
        let theirs = commit! {};

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir"), "dir contents");
        Ok(())
    })
}

#[test]
fn test_merge_blob_to_tree_into_blob_to_tree_no_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "bar"
            }
        };

        let theirs = commit! {
            foo {
                baz < "baz"
            }
        };

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "foo/bar"), "bar");
        assert_eq!(cat!(repo: "foo/baz"), "baz");
        Ok(())
    })
}

#[test]
fn test_merge_blob_to_tree_into_blob_to_tree_with_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            foo {
                bar < "bar"
            }
        };

        let theirs = commit! {
            foo {
                bar {
                    baz < "baz"
                }
            }
        };

        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        let mut conflicts = conflicts.into_iter();
        assert_eq!(
            conflicts.next().unwrap(),
            Conflict::new_with_type(p!("foo/bar"), ConflictType::AddedByUs)
        );
        assert_eq!(cat!(repo: "foo/bar~HEAD"), "bar");
        assert_eq!(cat!(repo: "foo/bar/baz"), "baz");
        Ok(())
    })
}

#[test]
fn test_merge_tree_to_blob_into_tree_to_blob_with_conflict() -> BitResult<()> {
    BitRepo::with_minimal_repo_with_dir(|repo| {
        let ours = commit! {
            dir < "dir"
        };

        let theirs = commit! {
            dir < "conflict"
        };

        let conflicts = repo.three_way_merge(ours, theirs)?.into_conflicts();
        assert_eq!(conflicts[0], Conflict::new_with_type(p!("dir"), ConflictType::BothAdded));
        Ok(())
    })
}

#[test]
fn test_merge_both_created_tree() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            dir {
                foo < "foo"
            }
        };

        let theirs = commit! {
            dir {
                foo < "foo"
                bar < "bar"
            }
        };

        repo.three_way_merge(ours, theirs)?;
        assert_eq!(cat!(repo: "dir/foo"), "foo");
        assert_eq!(cat!(repo: "dir/bar"), "bar");
        Ok(())
    })
}

#[test]
fn test_conflicts_with_untracked() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            dir {
                foo < "foo"
            }
        };

        let theirs = commit! {
            conflict
        };

        repo.setup_three_way_merge(ours, theirs)?;

        touch!(repo: "conflict" < "maybe we could special case where the contents are the same and not conflict?");
        let conflicts = bit_merge!(repo: "theirs").unwrap_err().try_into_merge_conflict()?;
        let mut uncommitted = conflicts.uncommitted.into_iter();
        assert_eq!(uncommitted.next().unwrap(), p!("conflict"));
        assert_eq!(cat!(repo: "dir/foo"), "foo");
        Ok(())
    })
}

#[test]
fn test_conflicts_with_staged_changes() -> BitResult<()> {
    BitRepo::with_minimal_repo(|repo| {
        let ours = commit! {
            dir {
                foo < "foo"
            }
        };

        let theirs = commit! {};

        repo.setup_three_way_merge(ours, theirs)?;
        modify!(repo: "dir/foo" < "updated in index");

        let conflicts = bit_merge!(repo: "theirs").unwrap_err().try_into_merge_conflict()?;
        let mut uncommitted = conflicts.uncommitted.into_iter();
        assert_eq!(uncommitted.next().unwrap(), p!("dir/foo"));
        assert_eq!(cat!(repo: "dir/foo"), "foo", "the file on disk should retain its original");
        Ok(())
    })
}
