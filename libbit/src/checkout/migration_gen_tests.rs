use crate::checkout::Migration;
use crate::iter::BitEntry;
use crate::peel::Peel;
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

// #[test]
// fn test_migration_gen_on_sample_repo() -> BitResult<()> {
//     BitRepo::with_sample_repo(|repo| {
//         let old_tree_oid = repo.fully_resolve_rev(&rev!("HEAD^"))?.peel(repo)?.tree;
//         let old_iter = repo.tree_iter(old_tree_oid);
//         let new_iter = repo.head_tree_iter()?;
//         let migration = Migration::generate(old_iter, new_iter)?;

//         let expected = TestMigration {
//             rmrfs: vec![],
//             rms: vec![],
//             mkdirs: vec!["dir", "dir/bar"],
//             creates: vec!["dir/bar.l", "dir/bar/qux", "dir/baz", "dir/link"],
//         };

//         assert_eq!(migration, expected);

//         Ok(())
//     })
// }

// #[test]
// fn test_simple_migration_gen() -> BitResult<()> {
//     BitRepo::with_empty_repo(|repo| {
//         let a = tree! {
//             foo
//             bar {
//                 baz
//             }
//             qux {
//                 quxx
//             }
//         };

//         let b = tree! {
//            bar {
//                baz {
//                    c {
//                        d
//                    }
//                }
//            }
//            boo
//         };

//         let old_iter = repo.tree_iter(a);
//         let new_iter = repo.tree_iter(b);

//         let migration = Migration::generate(old_iter, new_iter)?;
//         let expected = TestMigration {
//             rmrfs: vec!["qux"],
//             rms: vec!["bar/baz", "foo"],
//             mkdirs: vec!["bar/baz", "bar/baz/c"],
//             creates: vec!["bar/baz/c/d", "boo"],
//         };

//         assert_eq!(migration, expected);
//         Ok(())
//     })
// }
