use crate::checkout::MigrationDiffer;
use crate::diff::TreeDiffBuilder;
use crate::error::BitResult;
use crate::repo::BitRepo;

#[test]
fn test_simple_migration_generation() -> BitResult<()> {
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

        let migration = MigrationDiffer::default().build_diff(old_iter, new_iter)?;
        dbg!(&migration);

        Ok(())
    })
}
