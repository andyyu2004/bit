use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;

#[derive(Clap, Debug, PartialEq)]
pub struct BitDiffCliOpts {
    #[clap(long = "staged")]
    // can't seem to get the `default_missing_value` to work so just nesting options instead
    // and create the default in code
    staged: Option<Option<BitRef>>,
}

impl Cmd for BitDiffCliOpts {
    fn exec(&self, repo: &BitRepo) -> BitResult<()> {
        let status = if let Some(r) = self.staged {
            let r = r.unwrap_or(BitRef::HEAD);
            repo.diff_head_index()
        } else {
            repo.diff_index_worktree()
        };
        Ok(())
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
            opts,
            BitDiffCliOpts {
                staged: Some(Some(BitRef::Symbolic(SymbolicRef::new(BitPath::intern("foo")))))
            }
        );

        let opts = BitDiffCliOpts::parse_from(&["--", "--staged"]);
        assert_eq!(opts, BitDiffCliOpts { staged: Some(None) });

        let opts = BitDiffCliOpts::parse_from(&["--"]);
        assert_eq!(opts, BitDiffCliOpts { staged: None });
    }
}
