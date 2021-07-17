mod revwalk;

pub use revwalk::*;

use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObjType, Oid, PartialOid};
use crate::refs::{BitRef, SymbolicRef};
use crate::repo::BitRepo;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::lazy::OnceCell;
use std::str::FromStr;

// <rev> ::= <rev>^ | <rev>~<num> | <ref>  | <partial-oid>
#[derive(Debug, Clone, PartialEq)]
pub enum Revspec {
    Ref(BitRef),
    Partial(PartialOid),
    Parent(Box<Revspec>),
    Ancestor(Box<Revspec>, u32),
}

impl<'rcx> BitRepo<'rcx> {
    /// resolve a revision to an oid
    pub fn fully_resolve_rev(self, rev: &LazyRevspec) -> BitResult<Oid> {
        let reference = self.resolve_rev(rev)?;
        self.fully_resolve_ref(reference)
    }

    /// resolve a revision to a reference (either a branch or a commit, never HEAD itself)
    pub fn resolve_rev(self, rev: &LazyRevspec) -> BitResult<BitRef> {
        self.fully_resolve_rev_to_ref(rev.parse(self)?)
    }

    pub fn resolve_rev_to_branch(self, rev: &LazyRevspec) -> BitResult<SymbolicRef> {
        match self.resolve_rev(rev)? {
            BitRef::Direct(..) => bail!("expected branch, found commit `{}`", rev),
            BitRef::Symbolic(sym) => Ok(sym),
        }
    }

    fn fully_resolve_rev_to_ref(&self, rev: &Revspec) -> BitResult<BitRef> {
        let get_parent = |reference: BitRef| -> BitResult<BitRef> {
            let oid = self.fully_resolve_ref(reference)?;
            let obj_type = self.read_obj_header(oid)?.obj_type;
            ensure_eq!(
                obj_type,
                BitObjType::Commit,
                "object `{}` is a {}, not a commit",
                oid,
                obj_type
            );
            let commit = self.read_obj(oid)?.into_commit();
            match &commit.parents[..] {
                [] => bail!("revision `{}` refers to the parent of an initial commit", rev),
                &[sole_parent] => Ok(BitRef::Direct(sole_parent)),
                _ => todo!(
                    "parent of multiple commits (i think git somewhat unintuitively takes the last parent? to confirm)"
                ),
            }
        };

        match *rev {
            // we want to resolve HEAD once
            Revspec::Ref(r) if r == BitRef::HEAD => self.read_head(),
            Revspec::Ref(r) => Ok(r),
            Revspec::Partial(prefix) => self.expand_prefix(prefix).map(BitRef::Direct),
            Revspec::Parent(ref inner) => self.fully_resolve_rev_to_ref(inner).and_then(get_parent),
            Revspec::Ancestor(ref rev, n) =>
                (0..n).try_fold(self.fully_resolve_rev_to_ref(&rev)?, |oid, _| get_parent(oid)),
        }
    }
}

impl Display for Revspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Revspec::Ref(r) => write!(f, "{}", r),
            Revspec::Partial(prefix) => write!(f, "{}", prefix),
            Revspec::Parent(rev) => write!(f, "{}^", rev),
            Revspec::Ancestor(rev, n) => write!(f, "{}~{}", rev, n),
        }
    }
}

// pretty weird wrapper around revspec
// problem is revspec requires repo to be properly evaluated (as it requires some context to be parsed properly)
// but we want FromStr to be implemented so clap can use it
// this wrapper can lazily evaluated to get a parsed revspec (via `parse`)
#[derive(Debug)]
pub struct LazyRevspec {
    src: String,
    parsed: OnceCell<Revspec>,
}

impl Display for LazyRevspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.src)
    }
}

impl LazyRevspec {
    pub fn parse(&self, repo: BitRepo<'_>) -> BitResult<&Revspec> {
        self.parsed.get_or_try_init(|| RevspecParser::new(repo, &self.src).parse())
    }
}

impl FromStr for LazyRevspec {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self { src: s.to_owned(), parsed: Default::default() })
    }
}

lazy_static! {
    static ref REV_SEPS: HashSet<char> = hashset! {
        '@', '~', '^'
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
    fn parse_base(&mut self) -> BitResult<Revspec> {
        let repo = self.repo;
        // some hacky special case for parsing the alias @ for HEAD
        // it's a bit annoying as @ is both a separator and a valid base
        let s = if &self.src[0..1] == "@" {
            self.src = &self.src[1..];
            "@"
        } else {
            self.next()?
        };

        // try to interpret as a ref first and if it parses, then expand it to see if it resolves to something valid
        // this is better than doing it as a partialoid first as partialoid might fail either due to being ambiguous or due to not existing
        // but refs will only fail for not existing
        // rev's are ambiguous
        // how can we tell if something is a partial oid or a valid reference (e.g. nothing prevents "abcd" from being both a valid prefix and valid branch name)
        // (if a branch happens to have the same name as a valid prefix then bad luck I guess? but seems quite unlikely in practice)
        if let Ok(r) = BitRef::from_str(s).and_then(|r| {
            // if the ref is not "fully resolvable" then
            repo.fully_resolve_ref(r)?;
            // we don't return the fully resolved ref as we want the original for better error messages
            // we are just checking if it is resolvable
            Ok(r)
        }) {
            Ok(Revspec::Ref(r))
        } else {
            PartialOid::from_str(s)
                .and_then(|prefix| repo.expand_prefix(prefix))
                .map(BitRef::from)
                .map(Revspec::Ref)
        }
    }

    fn expect_num(&mut self) -> BitResult<u32> {
        Ok(u32::from_str(self.next()?)?)
    }

    pub fn parse(mut self) -> BitResult<Revspec> {
        let mut rev = self.parse_base()?;
        while !self.src.is_empty() {
            let (c, cs) = self.src.split_at(1);
            self.src = cs;
            match c {
                "^" => rev = Revspec::Parent(Box::new(rev)),
                "~" => {
                    let n = self.expect_num()?;
                    rev = Revspec::Ancestor(Box::new(rev), n);
                }
                _ => bail!("unexpected token `{}`, while parsing revspec", c),
            }
        }
        Ok(rev)
    }
}

#[cfg(test)]
mod tests;
