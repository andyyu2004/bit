use super::Cmd;
use clap::Parser;
use libbit::checkout::CheckoutOpts;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;

#[derive(Parser, Debug, PartialEq)]
pub struct BitSwitchCliOpts {
    #[clap(short = 'c', long = "create")]
    create: Option<String>,
    /// The revision to checkout
    /// If -c is passed, then this revision becomes the starting point for the new branch
    #[clap(required_unless_present("create"), default_value = "HEAD")]
    revision: Revspec,
}

impl Cmd for BitSwitchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let target = if let Some(branch_name) = self.create {
            let new_branch = repo.bit_create_branch(&branch_name, &self.revision)?;
            println!("switched to a new branch `{}`", new_branch.short());

            // the "annoying" case of nonexistent branch, trying to check it out will fail.
            // the branch changing logic for this special case is handled in `create_branch`
            // so we are already done, so just exit early
            if repo.try_fully_resolve_ref(new_branch)?.is_none() {
                return Ok(());
            }
            new_branch
        } else {
            // switch is currently a limited form of checkout where only branches are allowed (can't checkout commits)
            repo.resolve_rev_to_branch(&self.revision)?
        };
        repo.checkout(target, CheckoutOpts::default())?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_switch_opts() {
        let opts = BitSwitchCliOpts::try_parse_from(&["HEAD"]).unwrap();
        assert_eq!(opts, BitSwitchCliOpts { create: None, revision: "HEAD".parse().unwrap() })
    }

    #[test]
    fn parse_switch_opts_create_branch_from_head() {
        let opts = BitSwitchCliOpts::try_parse_from(&["--", "-c", "some-branch"]).unwrap();
        assert_eq!(
            opts,
            BitSwitchCliOpts {
                create: Some("some-branch".to_owned()),
                revision: "HEAD".parse().unwrap()
            }
        )
    }

    #[test]
    fn parse_switch_opts_create_branch_from_revision() {
        let opts = BitSwitchCliOpts::try_parse_from(&["--", "-c", "some-branch", "@^4"]).unwrap();
        assert_eq!(
            opts,
            BitSwitchCliOpts {
                create: Some("some-branch".to_owned()),
                revision: "@^4".parse().unwrap()
            }
        )
    }
}
