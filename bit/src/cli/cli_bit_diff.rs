use super::Cmd;
use clap::Clap;
use libbit::diff::Apply;
use libbit::diff::Diff;
use libbit::diff::Differ;
use libbit::error::BitResult;
use libbit::index::BitIndex;
use libbit::index::BitIndexEntry;
use libbit::pathspec::Pathspec;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;

#[derive(Clap, Debug, PartialEq)]
pub struct BitDiffCliOpts {
    #[clap(long = "staged")]
    // can't seem to get the `default_missing_value` to work so just nesting options instead
    // and create the default in code
    staged: Option<Option<BitRef>>,
    pathspec: Option<Pathspec>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        let pathspec = self.pathspec.unwrap_or(Pathspec::MATCH_ALL);
        let status = if let Some(r) = self.staged {
            let r = r.unwrap_or(BitRef::HEAD);
            let tree = r.resolve_to_tree(repo)?;
            repo.diff_tree_index(&tree, pathspec)?
        } else {
            repo.diff_index_worktree(pathspec)?
        };

        struct DiffFormatter<'r> {
            repo: &'r BitRepo,
        }

        impl<'r> Apply for DiffFormatter<'r> {
            fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
                todo!()
            }

            fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
                todo!()
            }

            fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
                todo!()
            }
        }

        status.apply(&mut DiffFormatter { repo })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libbit::path::BitPath;
    use libbit::refs::SymbolicRef;

    #[test]
    fn test_cli_parse_bit_diff_staged() {
        let opts = BitDiffCliOpts::parse_from(&["--", "--staged", "foo"]);
        assert_eq!(
            opts.staged,
            Some(Some(BitRef::Symbolic(SymbolicRef::new(BitPath::intern("foo"))))),
        );

        let opts = BitDiffCliOpts::parse_from(&["--", "--staged"]);
        assert_eq!(opts.staged, Some(None));

        let opts = BitDiffCliOpts::parse_from(&["--"]);
        assert_eq!(opts.staged, None);
    }
}
