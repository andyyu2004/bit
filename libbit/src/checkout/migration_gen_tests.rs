use crate::checkout::{CheckoutOpts, Migration};
use crate::error::BitResult;
use crate::iter::BitEntry;
use crate::peel::Peel;
use crate::repo::BitRepo;

// basically a simpler migration by holding strings (paths) instead of tree entries
// and is much easier to write up as a test case for equality
#[derive(Debug, PartialEq)]
struct TestMigration {
    rmrfs: Vec<&'static str>,
    rms: Vec<&'static str>,
    mkdirs: Vec<&'static str>,
    creates: Vec<&'static str>,
}

impl From<Migration> for TestMigration {
    fn from(migration: Migration) -> Self {
        assert!(migration.mkdirs.iter().all(|entry| entry.is_tree()));
        assert!(migration.rmrfs.iter().all(|entry| entry.is_tree()));
        assert!(migration.rms.iter().all(|entry| entry.is_blob()));
        assert!(migration.creates.iter().all(|entry| entry.is_blob()));

        let rmrfs = migration.rmrfs.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let rms = migration.rms.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let mkdirs = migration.mkdirs.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let creates = migration.creates.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();

        Self { rmrfs, rms, mkdirs, creates }
    }
}

#[test]
fn test_migration_gen_on_sample_repo() -> BitResult<()> {
    BitRepo::with_sample_repo(|repo| {
        let old_tree_oid = repo.fully_resolve_rev(&rev!("HEAD^"))?.peel(repo)?.tree;
        let old_iter = repo.tree_iter(old_tree_oid);
        let new_iter = repo.head_tree_iter()?;

        repo.with_index_mut(|index| {
            let worktree = index.worktree_iter()?;

            let expected = TestMigration {
                rmrfs: vec![],
                rms: vec![],
                mkdirs: vec!["dir", "dir/bar"],
                creates: vec!["dir/bar.l", "dir/bar/qux", "dir/baz", "dir/link"],
            };

            let safe_migration =
                Migration::generate(index, old_iter, new_iter, worktree, CheckoutOpts::default())?;
            assert_eq!(TestMigration::from(safe_migration), expected);

            Ok(())
        })
    })
}

#[test]
fn test_simple_migration_gen() -> BitResult<()> {
    BitRepo::with_empty_repo(|repo| {
        let a = tree! {
            foo
            bar {
                baz
            }
            qux {
                quxx
            }
        };

        let b = tree! {
           bar {
               baz {
                   c {
                       d
                   }
               }
           }
           boo
        };

        let old_iter = repo.tree_iter(a);
        let new_iter = repo.tree_iter(b);

        repo.with_index_mut(|index| {
            let worktree = index.worktree_iter()?;
            let expected = TestMigration {
                rmrfs: vec!["qux"],
                rms: vec!["bar/baz", "foo"],
                mkdirs: vec!["bar/baz", "bar/baz/c"],
                creates: vec!["bar/baz/c/d", "boo"],
            };

            let migration =
                Migration::generate(index, old_iter, new_iter, worktree, CheckoutOpts::default())?;
            assert_eq!(TestMigration::from(migration), expected);
            Ok(())
        })
    })
}
