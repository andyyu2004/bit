use crate::diff::{Diff, Differ, WorkspaceStatus};
use crate::iter::BitEntry;
impl WorkspaceStatus {
    pub fn format_diff_into(
        repo: BitRepo<'rcx>,
        writer: W,
        status: &WorkspaceStatus,
    ) -> BitResult<()> {
        status.apply_with(&mut Self::new(repo, writer))
        let bytes = entry.read_to_bytes(self.repo)?;
        // TODO diffing binary files?
        // currently will just not do anything as they all have the same string representation
        Ok(String::from_utf8(bytes).unwrap_or_else(|_| "<binary>".to_string()))
impl<'rcx, W: Write> Differ for DiffFormatter<'rcx, W> {