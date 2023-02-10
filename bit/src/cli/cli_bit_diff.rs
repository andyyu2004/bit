use super::Cmd;
use clap::Parser;
use libbit::error::BitResult;
use libbit::pathspec::Pathspec;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;
use libbit::xdiff::DiffFormatExt;
use std::process::{Command, Stdio};

#[derive(Parser, Debug, PartialEq)]
pub struct BitDiffCliOpts {
    #[arg(long = "stat")]
    stat: bool,
    #[arg(long = "staged")]
    staged: bool,
    #[arg(num_args=..=2)]
    revs: Vec<Revspec>,
    // pathspec: Option<Pathspec>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(self, repo: BitRepo) -> BitResult<()> {
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
            diff.print_diffstat(&repo)?;
        } else {
            let mut pager = Command::new(repo.config().pager()).stdin(Stdio::piped()).spawn()?;
            diff.format_diff_into(&repo, pager.stdin.as_mut().unwrap())?;
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
        let opts = BitDiffCliOpts::parse_from(["--", "--staged", "foo"]);
        assert!(opts.staged);
        assert_eq!(opts.revs.len(), 1);

        let opts = BitDiffCliOpts::parse_from(["--", "--staged"]);
        assert!(opts.staged);
        assert!(opts.revs.is_empty());

        let opts = BitDiffCliOpts::parse_from(["--"]);
        assert!(!opts.staged);
        assert!(opts.revs.is_empty());
    }

    #[test]
    fn test_cli_parse_bit_diff_two_revs() {
        let opts = BitDiffCliOpts::parse_from(["--", "foo", "bar"]);
        assert!(!opts.staged);
        assert_eq!(opts.revs.len(), 2);
    }
}
