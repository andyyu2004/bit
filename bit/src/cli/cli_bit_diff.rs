use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::pathspec::Pathspec;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;
use std::process::{Command, Stdio};

#[derive(Clap, Debug, PartialEq)]
pub struct BitDiffCliOpts {
    #[clap(long = "stat")]
    stat: bool,
    #[clap(long = "staged")]
    staged: bool,
    #[clap(max_values = 2)]
    revs: Vec<Revspec>,
    // pathspec: Option<Pathspec>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        // let pathspec = self.pathspec.unwrap_or(Pathspec::MATCH_ALL);
        let pathspec = Pathspec::MATCH_ALL;
        let diff = match &self.revs[..] {
            [] =>
                if self.staged {
                    repo.diff_head_index(pathspec)?
                } else {
                    repo.diff_index_worktree(pathspec)?
                },
            [rev] => {
                let treeish_oid = repo.fully_resolve_rev_to_any(rev)?;
                if self.staged {
                    repo.diff_tree_index(treeish_oid, pathspec)?
                } else {
                    repo.diff_tree_worktree(treeish_oid, pathspec)?
                }
            }
            [a, b] => {
                ensure!(
                    !self.staged,
                    "`--staged` has no effect when passing two revisions to diff"
                );
                let a = repo.fully_resolve_rev_to_any(a)?;
                let b = repo.fully_resolve_rev_to_any(b)?;
                repo.diff_tree_to_tree(a, b)?
            }
            _ => unreachable!(),
        };

        if self.stat {
            diff.format_diffstat_into(repo, std::io::stdout())?;
        } else {
            let mut pager = Command::new(&repo.config().pager()).stdin(Stdio::piped()).spawn()?;
            diff.format_diff_into(repo, pager.stdin.as_mut().unwrap())?;
            pager.wait()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_bit_diff_staged() {
        let opts = BitDiffCliOpts::parse_from(&["--", "--staged", "foo"]);
        assert_eq!(opts.staged, true);
        assert_eq!(opts.revs.len(), 1);

        let opts = BitDiffCliOpts::parse_from(&["--", "--staged"]);
        assert_eq!(opts.staged, true);
        assert!(opts.revs.is_empty());

        let opts = BitDiffCliOpts::parse_from(&["--"]);
        assert_eq!(opts.staged, false);
        assert!(opts.revs.is_empty());
    }

    #[test]
    fn test_cli_parse_bit_diff_two_revs() {
        let opts = BitDiffCliOpts::parse_from(&["--", "foo", "bar"]);
        assert_eq!(opts.staged, false);
        assert_eq!(opts.revs.len(), 2);
    }
}
