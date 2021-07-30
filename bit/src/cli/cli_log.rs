use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::format::{Indentable, OwoColorize};
use libbit::iter::FallibleIterator;
use libbit::obj::{BitObject, Oid};
use libbit::refs::{BitRef, Refs, SymbolicRef, SymbolicRefKind};
use libbit::repo::BitRepo;
use libbit::rev::LazyRevspec;
use owo_colors::colors::*;
use owo_colors::{Style, Styled};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::{self, Display, Formatter};
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

        let refs = repo.ls_refs()?;
        let decorations_map = calculate_decorations(repo, &refs)?;

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

trait SymbolicRefStyleExt: Sized + Copy {
    fn styled<T>(self, value: T) -> Styled<T>;
    fn short_styled(self) -> Styled<&'static str>;
}

impl SymbolicRefStyleExt for SymbolicRef {
    fn styled<T>(self, value: T) -> owo_colors::Styled<T> {
        let style = Style::default();
        match self.kind() {
            SymbolicRefKind::Head => style.fg::<BrightCyan>(),
            SymbolicRefKind::Branch => style.fg::<Green>(),
            SymbolicRefKind::Tag => todo!(),
            SymbolicRefKind::Remote => style.fg::<Red>(),
            SymbolicRefKind::Unknown => unreachable!(),
        }
        .bold()
        .style(value)
    }

    fn short_styled(self) -> Styled<&'static str> {
        self.styled(self.short())
    }
}

// NOTE: our decoration diverges from git a bit
// we use the -> decoration for all symbolic references
// git doesn't do it for origin/HEAD -> origin/master for example
#[derive(Debug, Eq, PartialEq)]
enum RefDecoration {
    Branch(SymbolicRef),
    /// outer ref pointing to inner ref
    /// HEAD -> refs/heads/master
    /// symbolic_ref -> branch
    Symbolic(SymbolicRef, SymbolicRef),
}

impl PartialOrd for RefDecoration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RefDecoration {
    fn cmp(&self, other: &Self) -> Ordering {
        // somewhat arbitrary, used for ordering the decorations
        // symbolic refs come first, otherwise defer to the ordering of the decoration style
        // least comes first in btreeset iterator
        match (self, other) {
            (RefDecoration::Branch(a), RefDecoration::Branch(b)) => a.kind().cmp(&b.kind()),
            (RefDecoration::Branch(..), RefDecoration::Symbolic(..)) => Ordering::Greater,
            (RefDecoration::Symbolic(..), RefDecoration::Branch(..)) => Ordering::Less,
            (RefDecoration::Symbolic(a, _), RefDecoration::Symbolic(b, _)) =>
                a.kind().cmp(&b.kind()),
        }
    }
}

impl Display for RefDecoration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RefDecoration::Branch(branch) => write!(f, "{}", branch.short_styled()),
            RefDecoration::Symbolic(symbolic, branch) => write!(
                f,
                "{} {} {}",
                symbolic.short_styled(),
                "->".bright_cyan(),
                branch.short_styled()
            ),
        }
    }
}

fn calculate_decoration(repo: BitRepo<'_>, sym: SymbolicRef) -> BitResult<(Oid, RefDecoration)> {
    match repo.partially_resolve_ref(sym)? {
        BitRef::Direct(oid) => Ok((oid, RefDecoration::Branch(sym))),
        BitRef::Symbolic(inner_sym) => match repo.resolve_ref(inner_sym)? {
            BitRef::Direct(oid) => Ok((oid, RefDecoration::Symbolic(sym, inner_sym))),
            BitRef::Symbolic(_) => todo!("double symbolic ref"),
        },
    }
}

fn calculate_decorations(
    repo: BitRepo<'_>,
    refs: &Refs,
) -> BitResult<HashMap<Oid, BTreeSet<RefDecoration>>> {
    let mut decorations = HashMap::<Oid, BTreeSet<RefDecoration>>::new();
    let mut handled = HashSet::new();
    for sym in refs {
        // avoid printing out `HEAD -> master` and then `master` again
        // we know that `refs` is in order (as it's a btreeset) and therefore symbolic refs
        // come first (as HEAD and remotes are ordered before branches and tags)
        // and therefore the following check is sufficient
        // (i.e. it is not possible for a branch decoration to already be in `decorations`)
        if handled.contains(sym) {
            continue;
        }
        let (oid, decoration) = calculate_decoration(repo, *sym)?;
        if let RefDecoration::Symbolic(_, branch) = decoration {
            handled.insert(branch);
        }
        decorations.entry(oid).or_default().insert(decoration);
    }
    Ok(decorations)
}
