use std::collections::{BTreeSet, HashMap, HashSet};

use super::*;
use owo_colors::colors::*;
use owo_colors::{OwoColorize, Style, Styled};

impl SymbolicRef {
    fn styled<T>(self, value: T) -> owo_colors::Styled<T> {
        let style = Style::default();
        match self.kind() {
            SymbolicRefKind::Head => style.fg::<BrightCyan>(),
            SymbolicRefKind::Branch => style.fg::<Green>(),
            SymbolicRefKind::Tag => todo!(),
            SymbolicRefKind::Remote => style.fg::<Red>(),
            SymbolicRefKind::Unknown => unreachable!(),
            SymbolicRefKind::Stash => todo!(),
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
pub enum RefDecoration {
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
            (RefDecoration::Branch(a), RefDecoration::Branch(b)) =>
                a.kind().cmp(&b.kind()).then_with(|| a.cmp(b)),
            (RefDecoration::Branch(..), RefDecoration::Symbolic(..)) => Ordering::Greater,
            (RefDecoration::Symbolic(..), RefDecoration::Branch(..)) => Ordering::Less,
            (RefDecoration::Symbolic(a, x), RefDecoration::Symbolic(b, y)) =>
                a.kind().cmp(&b.kind()).then_with(|| a.cmp(b)).then_with(|| x.cmp(y)),
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

impl BitRepo<'_> {
    /// Given a ordered set of references, returns a map from a commit oid to a ordered set of it's decorations
    pub fn ref_decorations(self, refs: &Refs) -> BitResult<HashMap<Oid, BTreeSet<RefDecoration>>> {
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
            let (oid, decoration) = self.calculate_decoration(*sym)?;
            if let RefDecoration::Symbolic(_, branch) = decoration {
                handled.insert(branch);
            }
            assert!(decorations.entry(oid).or_default().insert(decoration));
        }
        Ok(decorations)
    }

    fn calculate_decoration(self, sym: SymbolicRef) -> BitResult<(Oid, RefDecoration)> {
        match self.partially_resolve_ref(sym)? {
            BitRef::Direct(oid) => Ok((oid, RefDecoration::Branch(sym))),
            BitRef::Symbolic(inner_sym) => match self.resolve_ref(inner_sym)? {
                BitRef::Direct(oid) => Ok((oid, RefDecoration::Symbolic(sym, inner_sym))),
                BitRef::Symbolic(_) => todo!("double symbolic ref"),
            },
        }
    }
}
