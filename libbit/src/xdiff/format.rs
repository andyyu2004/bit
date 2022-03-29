use super::*;
use crate::diff::{Diff, Differ, WorkspaceStatus};
use crate::error::BitResult;
use crate::index::BitIndexEntry;
use crate::iter::BitEntry;
use crate::obj::Oid;
use crate::path::BitPath;
use crate::repo::BitRepo;
use crate::xdiff;
use owo_colors::OwoColorize;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};

pub struct DiffFormatter<W> {
    repo: BitRepo,
    writer: W,
}

pub trait DiffFormatExt: Sized {
    fn format_diffstat_into(self, repo: BitRepo, writer: impl Write) -> BitResult<()>;
    fn format_diff_into(self, repo: BitRepo, writer: impl Write) -> BitResult<()>;

    fn print_diffstat(self, repo: BitRepo) -> BitResult<()> {
        self.format_diffstat_into(repo, std::io::stdout())
    }
}

impl WorkspaceStatus {
    pub fn print_change_summary(&self) -> BitResult<()> {
        for created in &self.new {
            println!("create mode {} {}", created.mode, created.path);
        }

        for deleted in &self.deleted {
            println!("delete mode {} {}", deleted.mode, deleted.path);
        }
        Ok(())
    }
}

impl<D: Diff> DiffFormatExt for D {
    fn format_diffstat_into(self, repo: BitRepo, writer: impl Write) -> BitResult<()> {
        DiffStatFormatter::format_diffstat_into(repo, writer, self)
    }

    fn format_diff_into(self, repo: BitRepo, writer: impl Write) -> BitResult<()> {
        DiffFormatter::format_diff_into(repo, writer, self)
    }
}

impl<W: Write> DiffFormatter<W> {
    pub fn new(repo: BitRepo, writer: W) -> Self {
        Self { repo, writer }
    }

    pub fn format_diff_into(repo: BitRepo, writer: W, status: impl Diff) -> BitResult<()> {
        status.apply_with(&mut Self::new(repo, writer))
    }
}

impl<W: Write> Differ for DiffFormatter<W> {
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        let new_txt = new.read_to_bytes(self.repo)?;
        let mut patch = xdiff::xdiff(&[], &new_txt);

        let a: BitPath = BitPath::A.join(new.path);
        let b: BitPath = BitPath::B.join(new.path);
        let writer = &mut self.writer;
        writeln!(writer, "diff --git {} {}", a, b)?;
        writeln!(writer, "new file mode {}", new.mode)?;
        writeln!(writer, "index {:#}..{:#}", Oid::UNKNOWN, new.oid)?;

        patch.set_original(Cow::Borrowed(b"/dev/null"));
        patch.set_modified(Cow::Borrowed(b.as_bytes()));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        debug_assert!(old.oid.is_known());
        let old_txt = old.read_to_bytes(self.repo)?;
        let new_txt = new.read_to_bytes(self.repo)?;
        let mut patch = xdiff::xdiff(&old_txt, &new_txt);
        let a: BitPath = BitPath::A.join(old.path);
        let b: BitPath = BitPath::B.join(new.path);

        let writer = &mut self.writer;
        writeln!(writer, "diff --git {} {}", a, b)?;

        let new_oid = if new.oid.is_known() {
            new.oid
        } else {
            self.repo.hash_blob_from_worktree(new.path)?
        };
        // TODO what if the file has changed mode?
        writeln!(writer, "index {:#}..{:#} {}", old.oid, new_oid, new.mode)?;
        patch.set_original(Cow::Borrowed(a.as_bytes()));
        patch.set_modified(Cow::Borrowed(b.as_bytes()));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        debug_assert!(old.oid.is_known());
        let old_txt = old.read_to_bytes(self.repo)?;
        let mut patch = xdiff::xdiff(&old_txt, &[]);

        let a: BitPath = BitPath::A.join(old.path);
        let b: BitPath = BitPath::B.join(old.path);
        let writer = &mut self.writer;
        writeln!(writer, "diff --git {} {}", a, b)?;
        writeln!(writer, "deleted file mode {}", old.mode)?;
        writeln!(writer, "index {:#}..{:#}", old.oid, Oid::UNKNOWN)?;
        patch.set_original(Cow::Borrowed(a.as_bytes()));
        patch.set_modified(Cow::Borrowed(b"/dev/null"));
        xdiff::format_patch_into(writer, &patch)?;
        Ok(())
    }
}

pub struct DiffStatFormatter<W> {
    repo: BitRepo,
    writer: W,
    diffstat_lines: BTreeSet<DiffStatLine>,
    max_path_len: usize,
    max_changes: usize,
    total_insertions: usize,
    total_deletions: usize,
}

impl<W: Write> DiffStatFormatter<W> {
    pub fn new(repo: BitRepo, writer: W) -> Self {
        Self {
            repo,
            writer,
            max_path_len: 0,
            max_changes: 0,
            total_insertions: 0,
            total_deletions: 0,
            diffstat_lines: Default::default(),
        }
    }

    // this api is a bit weird, why not just generate a diffstat struct from the workspace status
    // could just do a `diffstat` method on workspace status and then the user can write as one wishes
    pub fn format_diffstat_into(repo: BitRepo, writer: W, status: impl Diff) -> BitResult<()> {
        let mut this = Self::new(repo, writer);
        status.apply_with(&mut this)?;

        let lines = DiffStat {
            lines: std::mem::take(&mut this.diffstat_lines),
            max_changes: this.max_changes,
            max_path_len: this.max_path_len,
            total_deletions: this.total_deletions,
            total_insertions: this.total_insertions,
        };
        write!(this.writer, "{}", lines)?;
        Ok(())
    }

    fn add_line(&mut self, line: DiffStatLine) {
        self.max_path_len = std::cmp::max(self.max_path_len, line.path.len());
        self.max_changes = std::cmp::max(self.max_changes, line.insertions + line.deletions);
        self.total_deletions += line.deletions;
        self.total_insertions += line.insertions;
        self.diffstat_lines.insert(line);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct DiffStat {
    lines: BTreeSet<DiffStatLine>,
    max_changes: usize,
    max_path_len: usize,
    total_deletions: usize,
    total_insertions: usize,
}

impl Display for DiffStat {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let line_width =
            terminal_size::terminal_size().map(|width| width.0.0 as usize).unwrap_or(80);
        let remaining_width = line_width - self.max_path_len - 10;
        let max_change_width = self.max_changes.to_string().len();

        let mut scale = 1.0;
        if self.max_changes > remaining_width {
            scale = remaining_width as f64 / self.max_changes as f64;
        }

        for line in &self.lines {
            let changes = line.insertions + line.deletions;
            let scaled_insertions = line.insertions as f64 * scale;
            let scaled_deletions = line.deletions as f64 * scale;

            writeln!(
                f,
                " {} {}| {}{} {}{}",
                line.path,
                " ".repeat(self.max_path_len - line.path.len()),
                " ".repeat(max_change_width - changes.to_string().len()),
                changes,
                "+".repeat(scaled_insertions.ceil() as usize).green(),
                "-".repeat(scaled_deletions.ceil() as usize).red(),
            )?;
        }

        let &Self { total_insertions, total_deletions, .. } = self;
        write!(
            f,
            " {} files changed, {} insertion{}(+), {} deletion{}(-)",
            self.lines.len(),
            total_insertions,
            pluralize!(total_insertions),
            total_deletions,
            pluralize!(total_deletions),
        )
    }
}

#[derive(Debug, PartialEq, Copy, Clone, PartialOrd, Ord, Eq)]
pub struct DiffStatLine {
    // ensure this field is first so the ordering derivation is correct
    path: BitPath,
    insertions: usize,
    deletions: usize,
}

impl DiffStatLine {
    pub fn from_patch(path: BitPath, patch: &BitPatch<'_>) -> Self {
        let mut deletions = 0;
        let mut insertions = 0;
        for hunk in patch.hunks() {
            for line in hunk.lines() {
                match line {
                    diffy::Line::Context(..) => {}
                    diffy::Line::Delete(..) => deletions += 1,
                    diffy::Line::Insert(..) => insertions += 1,
                }
            }
        }
        Self { insertions, deletions, path }
    }
}

// TODO handle binary files better
// e.g. libbit/tests/repos/indextest/.bit/index | Bin 633 -> 652 bytes
impl<W: Write> Differ for DiffStatFormatter<W> {
    fn on_created(&mut self, new: BitIndexEntry) -> BitResult<()> {
        let new_txt = new.read_to_bytes(self.repo)?;
        let patch = xdiff::xdiff(&[], &new_txt);
        let diff_stat_line = DiffStatLine::from_patch(new.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }

    fn on_modified(&mut self, old: BitIndexEntry, new: BitIndexEntry) -> BitResult<()> {
        let old_txt = old.read_to_bytes(self.repo)?;
        let new_txt = new.read_to_bytes(self.repo)?;
        let patch = xdiff::xdiff(&old_txt, &new_txt);
        let diff_stat_line = DiffStatLine::from_patch(old.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }

    fn on_deleted(&mut self, old: BitIndexEntry) -> BitResult<()> {
        let old_txt = old.read_to_bytes(self.repo)?;
        let patch = xdiff::xdiff(&old_txt, &[]);
        let diff_stat_line = DiffStatLine::from_patch(old.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }
}
