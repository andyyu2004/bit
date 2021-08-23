use owo_colors::OwoColorize;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
    pub fn format_diffstat_into(&self, repo: BitRepo<'_>, writer: impl Write) -> BitResult<()> {
        DiffStatFormatter::format_diffstat_into(repo, writer, self)
    }

    pub fn format_diff_into(&self, repo: BitRepo<'_>, writer: impl Write) -> BitResult<()> {
        let new_txt = new.read_to_string(self.repo)?;
        let old_txt = old.read_to_string(self.repo)?;
        let new_txt = new.read_to_string(self.repo)?;
        let old_txt = old.read_to_string(self.repo)?;

pub struct DiffStatFormatter<'rcx, W> {
    repo: BitRepo<'rcx>,
    writer: W,
    diffstat_lines: BTreeSet<DiffStatLine>,
    max_path_len: usize,
    max_changes: usize,
    total_insertions: usize,
    total_deletions: usize,
}

impl<'rcx, W: Write> DiffStatFormatter<'rcx, W> {
    pub fn new(repo: BitRepo<'rcx>, writer: W) -> Self {
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

    pub fn format_diffstat_into(
        repo: BitRepo<'rcx>,
        writer: W,
        status: &WorkspaceStatus,
    ) -> BitResult<()> {
        let mut this = Self::new(repo, writer);
        status.apply_with(&mut this)?;

        let lines = DiffStatLines {
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
pub struct DiffStatLines {
    lines: BTreeSet<DiffStatLine>,
    max_changes: usize,
    max_path_len: usize,
    total_deletions: usize,
    total_insertions: usize,
}

impl Display for DiffStatLines {
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

impl<'rcx, W: Write> Differ for DiffStatFormatter<'rcx, W> {
    fn on_created(&mut self, new: &BitIndexEntry) -> BitResult<()> {
        let new_txt = new.read_to_string(self.repo)?;
        let patch = xdiff::xdiff("", &new_txt);
        let diff_stat_line = DiffStatLine::from_patch(new.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }

    fn on_modified(&mut self, old: &BitIndexEntry, new: &BitIndexEntry) -> BitResult<()> {
        let old_txt = old.read_to_string(self.repo)?;
        let new_txt = new.read_to_string(self.repo)?;
        let patch = xdiff::xdiff(&old_txt, &new_txt);
        let diff_stat_line = DiffStatLine::from_patch(old.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }

    fn on_deleted(&mut self, old: &BitIndexEntry) -> BitResult<()> {
        let old_txt = old.read_to_string(self.repo)?;
        let patch = xdiff::xdiff(&old_txt, "");
        let diff_stat_line = DiffStatLine::from_patch(old.path, &patch);
        self.add_line(diff_stat_line);
        Ok(())
    }
}