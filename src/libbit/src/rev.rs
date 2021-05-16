use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObjType, Oid};
use crate::refs::BitRef;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::iter::FromIterator;
use std::str::FromStr;

//
// <rev> = <rev>^ | <rev>~<num> | <ref>
#[derive(Debug, Clone, PartialEq)]
pub enum Revspec {
    Ref(BitRef),
    Parent(Box<Revspec>),
    Ancestor(Box<Revspec>, u32),
}

impl BitRepo {
    /// resolves revision specification to the commit oid
    pub fn resolve_rev(&self, rev: &Revspec) -> BitResult<Oid> {
        let get_parent = |oid: Oid| -> BitResult<Oid> {
            let commit = self.read_obj(oid)?.into_commit();
            match commit.parent {
                Some(parent) => Ok(parent),
                None => bail!("revision `{}` refers to the parent of an initial commit", rev),
            }
        };

        match rev {
            Revspec::Ref(r) => {
                let oid = self.resolve_ref(*r)?.try_into_oid()?;
                let obj_type = self.read_obj_header(oid)?.obj_type;
                ensure_eq!(
                    obj_type,
                    BitObjType::Commit,
                    "object `{}` is a {}, not a commit",
                    oid,
                    obj_type
                );
                Ok(oid)
            }
            Revspec::Parent(inner) => self.resolve_rev(inner).and_then(|oid| get_parent(oid)),
            Revspec::Ancestor(rev, n) =>
                (0..*n).try_fold(self.resolve_rev(rev)?, |oid, _| get_parent(oid)),
        }
    }
}

impl Display for Revspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Revspec::Ref(r) => write!(f, "{}", r),
            Revspec::Parent(rev) => write!(f, "{}^", rev),
            Revspec::Ancestor(rev, n) => write!(f, "{}~{}", rev, n),
        }
    }
}

impl FromStr for Revspec {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RevspecParser::new(s).parse()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum Token {
    Ref(BitRef),
    Num(u32),
    Caret,
    Tilde,
}

struct RevspecParser<'a> {
    src: &'a str,
    seps: HashSet<char>,
}

impl<'a> RevspecParser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, seps: HashSet::from_iter(std::array::IntoIter::new(['@', '~', '^'])) }
    }

    // moves src to the index of separator and returns the str before the separator
    fn next(&mut self) -> BitResult<&str> {
        let i = self.src.find(|c| self.seps.contains(&c)).unwrap_or_else(|| self.src.len());
        let s = &self.src[..i];
        self.src = &self.src[i..];
        Ok(s)
    }

    fn expect_ref(&mut self) -> BitResult<BitRef> {
        Ok(BitRef::from_str(self.next()?)?)
    }

    fn expect_num(&mut self) -> BitResult<u32> {
        Ok(u32::from_str(self.next()?)?)
    }

    pub fn parse(mut self) -> BitResult<Revspec> {
        let mut rev = Revspec::Ref(self.expect_ref()?);
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
