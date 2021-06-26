use super::*;
use crate::diff::{Apply, Diff, WorkspaceDiff};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::xdiff;
use std::borrow::Cow;

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
pub struct DiffFormatter<'rcx, W> {
    repo: BitRepo<'rcx>,
    writer: W,
}

impl WorkspaceDiff {
    pub fn format_into(&self, repo: BitRepo<'_>, writer: impl Write) -> BitResult<()> {
        DiffFormatter::format_diff_into(repo, writer, self)
    }
}

impl<'rcx, W: Write> DiffFormatter<'rcx, W> {
    pub fn new(repo: BitRepo<'rcx>, writer: W) -> Self {
        Self { repo, writer }
    }

    pub fn format_diff_into(repo: BitRepo<'rcx>, writer: W, diff: &WorkspaceDiff) -> BitResult<()> {
        diff.apply(&mut Self::new(repo, writer))
    }

    fn read_blob(&self, entry: &BitIndexEntry) -> BitResult<String> {
        if entry.oid.is_known() {
            // TODO diffing binary files?
            // currently the tostring impl will return the same thing
            // so if we textually diff it it won't show anything
            Ok(self.repo.read_obj(entry.oid)?.into_blob().to_string())
        } else {
            let absolute_path = self.repo.normalize(entry.path.as_path())?;
            Ok(std::fs::read_to_string(absolute_path)?)
        }
    }
}

impl<'rcx, W: Write> Apply for DiffFormatter<'rcx, W> {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
        let new_txt = self.read_blob(new)?;
        let mut patch = xdiff::xdiff("", &new_txt);

        let a = BitPath::A.join(new.path);
        let b = BitPath::B.join(new.path);
        let writer = &mut self.writer;
        writeln!(writer, "diff --bit {} {}", a, b)?;
        writeln!(writer, "new file mode {}", new.mode)?;
        writeln!(writer, "index {:#}..{:#}", Oid::UNKNOWN, new.oid)?;

        patch.set_original(Cow::Borrowed("/dev/null"));
        patch.set_modified(Cow::Borrowed(b.as_str()));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }

    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
        let old_txt = self.read_blob(old)?;
        let new_txt = self.read_blob(new)?;
        let mut patch = xdiff::xdiff(&old_txt, &new_txt);
        let a = BitPath::A.join(old.path);
        let b = BitPath::B.join(new.path);

        let writer = &mut self.writer;
        writeln!(writer, "diff --bit {} {}", a, b)?;
        // TODO what if the file has changed mode?
        writeln!(writer, "index {:#}..{:#} {}", old.oid, new.oid, new.mode)?;
        patch.set_original(Cow::Borrowed(a.as_str()));
        patch.set_modified(Cow::Borrowed(b.as_str()));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }

    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
        let old_txt = self.read_blob(old)?;
        let mut patch = xdiff::xdiff(&old_txt, "");

        let a = BitPath::A.join(old.path);
        let b = BitPath::B.join(old.path);
        let writer = &mut self.writer;
        writeln!(writer, "diff --bit {} {}", a, b)?;
        writeln!(writer, "deleted file mode {}", old.mode)?;
        writeln!(writer, "index {:#}..{:#}", old.oid, Oid::UNKNOWN)?;
        patch.set_original(Cow::Borrowed(a.as_str()));
        patch.set_modified(Cow::Borrowed("/dev/null"));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }
}
