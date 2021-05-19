use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObjType, Oid, PartialOid};
use crate::refs::BitRef;
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

impl BitRepo {
    pub fn resolve_rev(&self, rev: &LazyRevspec) -> BitResult<Oid> {
        self.resolve_rev_inner(rev.eval(self)?)
    }

    /// resolves revision specification to the commit oid
    fn resolve_rev_inner(&self, rev: &Revspec) -> BitResult<Oid> {
        let get_parent = |oid: Oid| -> BitResult<Oid> {
            let obj_type = self.read_obj_header(oid)?.obj_type;
            ensure_eq!(
                obj_type,
                BitObjType::Commit,
                "object `{}` is a {}, not a commit",
                oid,
                obj_type
            );
            let commit = self.read_obj(oid)?.into_commit();
            match commit.parent {
                Some(parent) => Ok(parent),
                None => bail!("revision `{}` refers to the parent of an initial commit", rev),
            }
        };

        match rev {
            Revspec::Ref(r) => self.fully_resolve_ref(*r),
            Revspec::Partial(prefix) => self.expand_prefix(*prefix),
            Revspec::Parent(inner) => self.resolve_rev_inner(inner).and_then(|oid| get_parent(oid)),
            Revspec::Ancestor(rev, n) =>
                (0..*n).try_fold(self.resolve_rev_inner(rev)?, |oid, _| get_parent(oid)),
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
// this wrapper can lazily evaluated to get a parsed revspec (via `eval`)
// obviously, this must only be done after tls::REPO is set
#[derive(Debug)]
pub struct LazyRevspec {
    src: String,
    parsed: OnceCell<Revspec>,
}

impl LazyRevspec {
    pub fn eval(&self, repo: &BitRepo) -> BitResult<&Revspec> {
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

struct RevspecParser<'a, 'r> {
    repo: &'r BitRepo,
    src: &'a str,
}

impl<'a, 'r> RevspecParser<'a, 'r> {
    pub fn new(repo: &'r BitRepo, src: &'a str) -> Self {
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
        let s = self.next()?;
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
