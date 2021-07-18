use super::*;
use crate::obj::{BitObject, Commit};
use fallible_iterator::FallibleIterator;
use std::fmt::{self, Display, Formatter};

pub struct RevList<'rcx> {
    revwalk: RevWalk<'rcx>,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn revlist(self, revspecs: &[&LazyRevspec]) -> BitResult<RevList<'rcx>> {
        let revwalk = self.revwalk(revspecs)?;
        Ok(RevList { revwalk })
    }
}

impl<'rcx> Display for RevList<'rcx> {
    // we want to do this "on demand" so we don't just hang for a while
    // and then output everything after processing
    // TODO cache the results if we have already done this
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut revwalk = self.revwalk.clone();
        while let Some(commit) =
            revwalk.next().expect("TODO error handling in display impl somehow")
        {
            writeln!(f, "{}", commit.oid())?;
        }
        Ok(())
    }
}
