use crate::checkout::{Migration, MigrationDiffer};
use crate::diff::TreeDiffBuilder;
use crate::error::BitResult;
use crate::iter::BitEntry;
use crate::path::BitPath;
use crate::repo::BitRepo;

// basically a simpler migration by holding strings (paths) instead of tree entries
// and is much easier to write up as a test case for equality
#[derive(Debug)]
struct TestMigration {
    rmrfs: Vec<&'static str>,
    rms: Vec<&'static str>,
    mkdirs: Vec<&'static str>,
    creates: Vec<&'static str>,
}

impl PartialEq<TestMigration> for Migration {
    fn eq(&self, other: &TestMigration) -> bool {
        assert!(self.mkdirs.iter().all(|entry| entry.is_tree()));
        assert!(self.rmrfs.iter().all(|entry| entry.is_tree()));
        assert!(self.rms.iter().all(|entry| entry.is_file()));
        assert!(self.creates.iter().all(|entry| entry.is_file()));

        let rmrfs = self.rmrfs.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let rms = self.rms.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let mkdirs = self.mkdirs.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        let creates = self.creates.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();

        rmrfs == other.rmrfs
            && rms == other.rms
            && mkdirs == other.mkdirs
            && creates == other.creates
    }
}

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
        let expected = TestMigration {
            rmrfs: vec!["qux"],
            rms: vec!["bar/baz", "foo"],
            mkdirs: vec!["bar/baz", "bar/baz/c"],
            creates: vec!["bar/baz/c/d", "boo"],
        };

        assert_eq!(migration, expected);
        Ok(())
    })
}
