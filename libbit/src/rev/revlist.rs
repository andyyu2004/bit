use super::*;
use crate::obj::{BitObject, Commit};
use fallible_iterator::FallibleIterator;
use std::fmt::{self, Display, Formatter};

pub struct RevList<'rcx> {
    commits: Vec<Commit<'rcx>>,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn revlist(self, revspecs: &[&LazyRevspec]) -> BitResult<RevList<'rcx>> {
        let revwalk = self.revwalk(revspecs)?;
        Ok(RevList { commits: revwalk.collect()? })
    }
}

impl<'rcx> Display for RevList<'rcx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for commit in &self.commits {
            writeln!(f, "{}", commit.oid())?;
        }
        Ok(())
    }
}
