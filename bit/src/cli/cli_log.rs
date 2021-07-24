use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::format::{Indentable, OwoColorize};
use libbit::iter::FallibleIterator;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clap, Debug)]
pub struct BitLogCliOpts {
    #[clap(default_value = "HEAD")]
    revisions: Vec<LazyRevspec>,
}

impl Cmd for BitLogCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let revisions = self.revisions.iter().collect::<Vec<_>>();
        let revwalk = repo.revwalk(&revisions)?;
        let mut pager = Command::new(&repo.config().pager()?).stdin(Stdio::piped()).spawn()?;
        let stdin = pager.stdin.as_mut().unwrap();
        revwalk.for_each(|commit| {
            writeln!(stdin, "{} {}", "commit".yellow(), commit.oid().yellow())?;
            writeln!(stdin, "Author: {} <{}>", commit.author.name, commit.author.email)?;
            writeln!(stdin, "Date: {}", commit.author.time)?;
            writeln!(stdin)?;
            writeln!(stdin, "{}", (&commit.message).indented("   "))?;
            writeln!(stdin)?;
            Ok(())
        })?;
        pager.wait()?;
        Ok(())
    }
}
