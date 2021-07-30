use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::pathspec::Pathspec;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;
use std::process::{Command, Stdio};

#[derive(Clap, Debug, PartialEq)]
pub struct BitDiffCliOpts {
    #[clap(long = "staged")]
    // can't seem to get the `default_missing_value` to work so just nesting options instead
    // and create the default in code
    staged: Option<Option<BitRef>>,
    pathspec: Option<Pathspec>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let pathspec = self.pathspec.unwrap_or(Pathspec::MATCH_ALL);
        let diff = if let Some(r) = self.staged {
            repo.diff_ref_index(r.unwrap_or(BitRef::HEAD), pathspec)?
        } else {
            repo.diff_index_worktree(pathspec)?
        };

        let mut pager = Command::new(&repo.config().pager()?).stdin(Stdio::piped()).spawn()?;
        diff.format_into(repo, pager.stdin.as_mut().unwrap())?;
        pager.wait()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libbit::refs::SymbolicRef;

    #[test]
    fn test_cli_parse_bit_diff_staged() {
        let opts = BitDiffCliOpts::parse_from(&["--", "--staged", "foo"]);
        assert_eq!(opts.staged, Some(Some(BitRef::Symbolic(SymbolicRef::intern("foo")))),);

        let opts = BitDiffCliOpts::parse_from(&["--", "--staged"]);
        assert_eq!(opts.staged, Some(None));

        let opts = BitDiffCliOpts::parse_from(&["--"]);
        assert_eq!(opts.staged, None);
    }
}
