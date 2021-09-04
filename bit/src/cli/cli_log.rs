use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::format::{Indentable, OwoColorize};
use libbit::iter::FallibleIterator;
use libbit::obj::BitObject;
use libbit::repo::BitRepo;
use libbit::rev::Revspec;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clap, Debug)]
pub struct BitLogCliOpts {
    #[clap(default_value = "HEAD")]
    revisions: Vec<Revspec>,
}

impl Cmd for BitLogCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        let revisions = self.revisions.iter().collect::<Vec<_>>();
        let revwalk = repo.revwalk(&revisions)?;
        let mut pager = Command::new(&repo.config().pager()).stdin(Stdio::piped()).spawn()?;
        let stdin = pager.stdin.as_mut().unwrap();

        let refs = repo.ls_refs()?;
        let decorations_map = repo.ref_decorations(&refs)?;

        revwalk.for_each(|commit| {
            write!(stdin, "{} {}", "commit".yellow(), commit.oid().yellow())?;
            if let Some(decorations) = decorations_map.get(&commit.oid()) {
                let s = decorations
                    .iter()
                    .map(|d| d.to_string())
                    .intersperse(", ".to_owned())
                    .collect::<String>();
                write!(stdin, " ({})", s)?;
            }
            writeln!(stdin)?;
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
