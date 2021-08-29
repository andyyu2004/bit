mod revwalk;

pub use revwalk::*;

use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObjType, Commit, Oid, PartialOid};
use crate::peel::Peel;
use crate::refs::{BitRef, BitRefDbBackend, SymbolicRef};
use crate::repo::{BitRepo, Repo};
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::lazy::OnceCell;
use std::str::FromStr;

// <rev> ::=
//   | <ref>
//   | <partial-oid>
//   | <rev>^<n>?
//   | <rev>~<n>?
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedRevspec {
    Ref(BitRef),
    Partial(PartialOid),
    /// nth parent selector ^2 means select the 2nd parent
    /// defaults to 1 if unspecified
    /// if n == 0, then this is a noop
    Parent(Box<ParsedRevspec>, usize),
    /// ~<n>
    Ancestor(Box<ParsedRevspec>, usize),
    /// <rev>@{<n>}
    Reflog(Box<ParsedRevspec>, usize),
}

impl<'rcx> BitRepo<'rcx> {
    /// resolve a revision to a commit oid
    pub fn fully_resolve_rev(self, rev: &Revspec) -> BitResult<Oid> {
        let reference = self.resolve_rev(rev)?;
        self.fully_resolve_ref(reference)
    }

    pub fn try_fully_resolve_rev(self, rev: &Revspec) -> BitResult<Option<Oid>> {
        let reference = self.resolve_rev(rev)?;
        self.try_fully_resolve_ref(reference)
    }

    /// resolve a revision to a reference (either a branch or a commit, never HEAD itself)
    pub fn resolve_rev(self, rev: &Revspec) -> BitResult<BitRef> {
        self.resolve_rev_internal(rev.parse(self)?)
    }

    pub fn resolve_rev_to_commit(self, rev: &Revspec) -> BitResult<&'rcx Commit<'rcx>> {
        self.fully_resolve_rev(rev)?.peel(self)
    }

    pub fn resolve_rev_to_branch(self, rev: &Revspec) -> BitResult<SymbolicRef> {
        match self.resolve_rev(rev)? {
            BitRef::Direct(..) => bail!("expected branch, found commit `{}`", rev),
            BitRef::Symbolic(sym) => Ok(sym),
        }
    }

    fn resolve_rev_internal(&self, rev: &ParsedRevspec) -> BitResult<BitRef> {
        let get_nth_parent = |reference, n| -> BitResult<BitRef> {
            let oid = self.fully_resolve_ref(reference)?;

            if n == 0 {
                return Ok(BitRef::Direct(oid));
            }

            let obj_type = self.read_obj_header(oid)?.obj_type;
            ensure_eq!(
                obj_type,
                BitObjType::Commit,
                "object `{}` is a {}, not a commit",
                oid,
                obj_type
            );

            let commit = self.read_obj_commit(oid)?;
            let parentc = commit.parents.len();

            if parentc == 0 {
                bail!("revision `{}` refers to the parent of an initial commit", rev)
            }

            // TODO testing nth parent selection once we have merging
            match commit.parents.get(n - 1) {
                Some(&parent) => Ok(BitRef::Direct(parent)),
                None => bail!(
                    "attempted to access parent {} (indexed starting from 1) of commit `{}` but it only has {} parent{}",
                    n,
                    oid,
                    parentc,
                    pluralize!(parentc),
                ),
            }
        };

        let get_first_parent = |reference| get_nth_parent(reference, 1);

        match *rev {
            // we want to resolve HEAD once
            ParsedRevspec::Ref(r) if r == BitRef::HEAD => self.read_head(),
            ParsedRevspec::Ref(r) => self.validate_ref(r),
            ParsedRevspec::Partial(prefix) => self.expand_prefix(prefix).map(BitRef::Direct),
            ParsedRevspec::Parent(ref inner, n) =>
                self.resolve_rev_internal(inner).and_then(|r| get_nth_parent(r, n)),
            ParsedRevspec::Ancestor(ref rev, n) =>
                (0..n).try_fold(self.resolve_rev_internal(rev)?, |oid, _| get_first_parent(oid)),
            ParsedRevspec::Reflog(ref inner, n) => match self.resolve_rev_internal(inner)? {
                BitRef::Direct(..) =>
                    bail!("can't use reflog revision syntax on a direct reference"),
                BitRef::Symbolic(sym) => {
                    let reflog = self.refdb()?.read_reflog(sym)?;
                    let entry = match reflog.get(n) {
                        Some(entry) => entry,
                        None => bail!(
                            "index `{}` is out of range in reflog with `{}` entries",
                            n,
                            reflog.len()
                        ),
                    };
                    Ok(BitRef::Direct(entry.new_oid))
                }
            },
        }
    }
}

impl Display for ParsedRevspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ParsedRevspec::Ref(r) => write!(f, "{}", r),
            ParsedRevspec::Partial(prefix) => write!(f, "{}", prefix),
            ParsedRevspec::Parent(rev, n) =>
                if *n == 1 {
                    write!(f, "{}^", rev)
                } else {
                    write!(f, "{}^{}", rev, n)
                },
            ParsedRevspec::Ancestor(rev, n) =>
                if *n == 1 {
                    write!(f, "{}^", rev)
                } else {
                    write!(f, "{}~{}", rev, n)
                },
            ParsedRevspec::Reflog(rev, n) => write!(f, "{}@{{{}}}", rev, n),
        }
    }
}

// pretty weird wrapper around revspec
// problem is revspec requires repo to be properly evaluated (as it requires some context to be parsed properly)
// but we want FromStr to be implemented so clap can use it
// this wrapper can lazily evaluated to get a parsed revspec (via `parse`)
#[derive(Debug, PartialEq)]
pub struct Revspec {
    src: String,
    parsed: OnceCell<ParsedRevspec>,
}

impl Display for Revspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.src)
    }
}

impl Revspec {
    pub fn parse(&self, repo: BitRepo<'_>) -> BitResult<&ParsedRevspec> {
        self.parsed.get_or_try_init(|| RevspecParser::new(repo, &self.src).parse())
    }
}

impl FromStr for Revspec {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self { src: s.to_owned(), parsed: Default::default() })
    }
}

lazy_static! {
    static ref REV_SEPS: HashSet<char> = hashset! {
        '@', '~', '^', '{', '}'
    };
}

struct RevspecParser<'a, 'rcx> {
    repo: BitRepo<'rcx>,
    src: &'a str,
}

impl<'a, 'rcx> RevspecParser<'a, 'rcx> {
    pub fn new(repo: BitRepo<'rcx>, src: &'a str) -> Self {
        Self { repo, src }
    }

    // moves src to the index of separator and returns the str before the separator
    fn next(&mut self) -> BitResult<&str> {
        let i = self.src.find(|c| REV_SEPS.contains(&c)).unwrap_or_else(|| self.src.len());
        let s = &self.src[..i];
        self.src = &self.src[i..];
        Ok(s)
    }

    /// either a partialoid or a ref
    fn parse_base(&mut self) -> BitResult<ParsedRevspec> {
        let repo = self.repo;
        // some hacky special case for parsing the alias @ for HEAD
        // it's a bit annoying as @ is both a separator and a valid base
        let s = if &self.src[0..1] == "@" {
            self.src = &self.src[1..];
            "@"
        } else {
            self.next()?
        };

        // try parse as a `partial_oid` first and try expand it
        // otherwise just parse it as a ref (either symbolic or direct)
        // there is no guarantee the ref is valid
        let reference = if let Ok(r) =
            PartialOid::from_str(s).and_then(|prefix| repo.expand_prefix(prefix)).map(BitRef::from)
        {
            r
        } else {
            BitRef::from_str(s)?
        };

        Ok(ParsedRevspec::Ref(reference))
    }

    fn expect(&mut self, s: &str) -> BitResult<()> {
        let n = s.len();
        if &self.src[..n] == s {
            self.src = &self.src[n..];
            Ok(())
        } else {
            bail!("expected `{}`, found `{}`", s, &self.src[..n])
        }
    }

    fn expect_num(&mut self) -> BitResult<usize> {
        Ok(usize::from_str(self.next()?)?)
    }

    fn accept_num(&mut self) -> Option<usize> {
        self.expect_num().ok()
    }

    pub fn parse(mut self) -> BitResult<ParsedRevspec> {
        let mut rev = self.parse_base()?;
        while !self.src.is_empty() {
            let (c, cs) = self.src.split_at(1);
            self.src = cs;
            match c {
                "^" => {
                    let n = self.accept_num().unwrap_or(1);
                    rev = ParsedRevspec::Parent(Box::new(rev), n)
                }
                "~" => {
                    let n = self.accept_num().unwrap_or(1);
                    rev = ParsedRevspec::Ancestor(Box::new(rev), n);
                }
                "@" => {
                    self.expect("{")?;
                    let n = self.expect_num()?;
                    self.expect("}")?;
                    rev = ParsedRevspec::Reflog(Box::new(rev), n);
                }
                _ => bail!("unexpected token `{}`, while parsing revspec", c),
            }
        }
        Ok(rev)
    }
}

#[cfg(test)]
mod revwalk_tests;
#[cfg(test)]
mod tests;
