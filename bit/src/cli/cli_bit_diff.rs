use std::borrow::Cow;
use std::io::Write;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use super::Cmd;
use clap::Clap;
use libbit::diff::Apply;
use libbit::diff::Diff;
use libbit::error::BitResult;
use libbit::index::BitIndexEntry;
use libbit::path::BitPath;
use libbit::pathspec::Pathspec;
use libbit::refs::BitRef;
use libbit::repo::BitRepo;
use libbit::xdiff;

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
        let status = if let Some(r) = self.staged {
            let r = r.unwrap_or(BitRef::HEAD);
            let oid = r.fully_resolve(repo)?;
            repo.diff_tree_index(oid, pathspec)?
        } else {
            repo.diff_index_worktree(pathspec)?
        };

        // NOTES:
        // don't know how correct this reasoning is
        // where to read the blob from given a `BitIndexEntry` `entry`?
        // if `entry.hash.is_unknown()` then it must be a worktree entry as otherwise the hash
        // would be definitely known.
        // however, does the converse hold? I think it currently does. Even though hashes for worktree entries
        // maybe sometimes be calculated due to racy git, I don't think the change is recorded in the entry we access
        // in the Apply trait.
        // if this is the case, we could just have two cases
        // - if the hash is known, then we read it from the object store,
        // - otherwise, we read it from disk
        struct DiffFormatter<'r> {
            repo: BitRepo<'r>,
            pager: Child,
        }

        impl<'r> DiffFormatter<'r> {
            pub fn new(repo: BitRepo<'r>) -> BitResult<Self> {
                let pager = Command::new(&repo.config().pager()?).stdin(Stdio::piped()).spawn()?;
                Ok(Self { repo, pager })
            }

            fn pipe(&mut self) -> impl Write + '_ {
                self.pager.stdin.as_mut().unwrap()
            }
        }

        impl<'r> DiffFormatter<'r> {
            fn read_blob(&self, entry: &BitIndexEntry) -> BitResult<String> {
                if entry.oid.is_known() {
                    // TODO diffing binary files?
                    // currently the tostring impl will return the same thing
                    // so if we textually diff it it won't show anything
                    Ok(self.repo.read_obj(entry.oid)?.into_blob().to_string())
                } else {
                    let absolute_path = self.repo.normalize(entry.path)?;
                    Ok(std::fs::read_to_string(absolute_path)?)
                }
            }
        }

        impl<'r> Apply for DiffFormatter<'r> {
            fn on_created(&mut self, _new: &BitIndexEntry) -> BitResult<()> {
                todo!()
            }

            fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
                let old_txt = self.read_blob(old)?;
                let new_txt = self.read_blob(new)?;
                let mut patch = xdiff::xdiff(&old_txt, &new_txt);
                let a = BitPath::A.join(old.path).as_str();
                let b = BitPath::B.join(new.path).as_str();
                writeln!(self.pipe(), "diff --bit {} {}", a, b)?;
                patch.set_original(Cow::Borrowed(a));
                patch.set_modified(Cow::Borrowed(b));
                xdiff::format_patch_into(self.pipe(), &patch)?;
                Ok(())
            }

            fn on_deleted(&mut self, _old: &BitIndexEntry) -> BitResult<()> {
                todo!()
            }
        }

        let mut formatter: DiffFormatter = DiffFormatter::new(repo)?;
        status.apply(&mut formatter)?;
        formatter.pager.wait()?;
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
            opts.staged,
            Some(Some(BitRef::Symbolic(SymbolicRef::new(BitPath::intern("foo"))))),
        );

        let opts = BitDiffCliOpts::parse_from(&["--", "--staged"]);
        assert_eq!(opts.staged, Some(None));

        let opts = BitDiffCliOpts::parse_from(&["--"]);
        assert_eq!(opts.staged, None);
    }
}
