use crate::error::{BitGenericError, BitResult};
use crate::obj::{BitObjType, Oid};
use crate::refs::BitRef;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::{self, Display, Formatter};
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
                let oid = self.resolve_ref(*r)?;
                let obj_type = self.read_obj_header(oid)?.obj_type;
                ensure_eq!(
                    obj_type,
                    BitObjType::Commit,
                    "object `{}` is a `{}`, not a commit",
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

lazy_static! {
    /// a ref is either a hash (partial?) or a symbolic ref
    // TODO this regex is pretty rough
    static ref REF_REGEX: Regex = Regex::new("^([a-zA-Z][a-zA-Z0-9]+)").unwrap();
    static ref NUM_REGEX: Regex = Regex::new(r#"^(\d+)"#).unwrap();
}

struct RevspecLexer<'a> {
    src: &'a str,
}

impl<'a> RevspecLexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src }
    }
}

impl<'a> FallibleIterator for RevspecLexer<'a> {
    type Error = BitGenericError;
    type Item = Token;

    fn next(&mut self) -> BitResult<Option<Token>> {
        let mut ref_capture_locations = REF_REGEX.capture_locations();
        let mut num_capture_locations = NUM_REGEX.capture_locations();
        if self.src.is_empty() {
            return Ok(None);
        } else if let Some(capture) = REF_REGEX.captures_read(&mut ref_capture_locations, self.src)
        {
            let s = capture.as_str();
            self.src = &self.src[s.len()..];
            return BitRef::from_str(s).map(Token::Ref).map(Some);
        } else if let Some(capture) = NUM_REGEX.captures_read(&mut num_capture_locations, self.src)
        {
            let s = capture.as_str();
            self.src = &self.src[s.len()..];
            let n = s.parse::<u32>().expect("failed to parse valid number");
            return Ok(Some(Token::Num(n)));
        }

        let (x, xs) = self.src.split_at(1);
        self.src = xs;
        match x {
            "^" => Ok(Some(Token::Caret)),
            "~" => Ok(Some(Token::Tilde)),
            x => (bail!("invalid token `{}` found in  revspec", x)),
        }
    }
}

struct RevspecParser<'a> {
    lexer: RevspecLexer<'a>,
}

impl<'a> RevspecParser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { lexer: RevspecLexer::new(src) }
    }

    fn expect_ref(&mut self) -> BitResult<BitRef> {
        match self.lexer.next()? {
            Some(Token::Ref(r)) => Ok(r),
            Some(token) => bail!("expected ref, found `{:?}`", token),
            None => bail!("expected ref, found eof"),
        }
    }

    fn expect_num(&mut self) -> BitResult<u32> {
        match self.lexer.next()? {
            Some(Token::Num(n)) => Ok(n),
            Some(token) => bail!("expected num, found `{:?}`", token),
            None => bail!("expected num, found eof"),
        }
    }

    pub fn parse(mut self) -> BitResult<Revspec> {
        let mut rev = Revspec::Ref(self.expect_ref()?);
        loop {
            match self.lexer.next()? {
                Some(token) => match token {
                    Token::Caret => rev = Revspec::Parent(Box::new(rev)),
                    Token::Tilde => {
                        let n = self.expect_num()?;
                        rev = Revspec::Ancestor(Box::new(rev), n);
                    }
                    Token::Num(..) => bail!("expected `^` or `~` but found number"),
                    Token::Ref(..) => bail!("expected `^` or `~` but found another ref"),
                },
                None => return Ok(rev),
            }
        }
    }
}

#[cfg(test)]
mod tests;
