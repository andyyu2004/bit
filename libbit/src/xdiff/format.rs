use super::*;
use crate::diff::{Diff, Differ, WorkspaceStatus};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::iter::BitEntry;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::xdiff;
use std::borrow::Cow;

pub struct DiffFormatter<'rcx, W> {
    repo: BitRepo<'rcx>,
    writer: W,
}

impl WorkspaceStatus {
    pub fn format_into(&self, repo: BitRepo<'_>, writer: impl Write) -> BitResult<()> {
        DiffFormatter::format_diff_into(repo, writer, self)
    }
}

impl<'rcx, W: Write> DiffFormatter<'rcx, W> {
    pub fn new(repo: BitRepo<'rcx>, writer: W) -> Self {
        Self { repo, writer }
    }

    pub fn format_diff_into(
        repo: BitRepo<'rcx>,
        writer: W,
        status: &WorkspaceStatus,
    ) -> BitResult<()> {
        status.apply_with(&mut Self::new(repo, writer))
    }

    fn read_blob(&self, entry: &BitIndexEntry) -> BitResult<String> {
        let bytes = entry.read_to_bytes(self.repo)?;
        // TODO diffing binary files?
        // currently will just not do anything as they all have the same string representation
        Ok(String::from_utf8(bytes).unwrap_or_else(|_| "<binary>".to_string()))
    }
}

impl<'rcx, W: Write> Differ for DiffFormatter<'rcx, W> {
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
