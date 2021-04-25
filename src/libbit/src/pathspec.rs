use crate::error::{BitGenericError, BitResult};
use crate::iter::BitIterator;
use crate::path::BitPath;
use crate::tls;
use itertools::Itertools;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Pathspec {
    /// non-wildcard prefix
    /// up to either the first wildcard or the last slash
    pub prefix: BitPath,
    // pathspec: Vec<()>,
}

impl TryFrom<&str> for Pathspec {
    type Error = BitGenericError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}

impl Display for Pathspec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.prefix)
    }
}

impl Pathspec {
    // prefix is the section up to the first unescaped wildcard symbol
    fn find_prefix_end(s: &str) -> Option<usize> {
        let chars = s.chars().collect_vec();
        for (i, c) in chars.iter().cloned().enumerate() {
            if Pathspec::is_wildcard(c) && (i == 0 || chars.get(i - 1) != Some(&'\\')) {
                return Some(i);
            }
        }
        None
    }

    pub fn match_worktree(self) -> BitResult<impl BitIterator> {
        tls::REPO.with(|repo| self.match_iterator(repo.worktree_iter()?))
    }

    pub fn match_index(self) -> BitResult<impl BitIterator> {
        tls::with_index(|index| self.match_iterator(index.iter()))
    }

    // pub fn match_tree(
    //     &self,
    //     tree: &Tree,
    // ) -> impl BitIterator {
    // }

    // braindead implementation for now
    pub fn matches_path(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref().starts_with(self.prefix)
    }

    fn match_iterator(self, iterator: impl BitIterator) -> BitResult<impl BitIterator> {
        Ok(iterator.filter(move |entry| Ok(self.matches_path(entry.filepath))))
    }

    fn is_wildcard(c: char) -> bool {
        c == '*' || c == '?' || c == '['
    }
}

pub struct FnMatch {
    path: BitPath,
    parent: BitPath,
}

impl FromStr for Pathspec {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "." {
            return Ok(Self { prefix: BitPath::intern("") });
        }
        let prefix_end = Self::find_prefix_end(&s);
        let prefix = match prefix_end {
            Some(i) => &s[..i],
            None => s,
        };
        Ok(Self { prefix: BitPath::intern(prefix) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BitResult;

    #[test]
    pub fn pathspec_prefix_test() -> BitResult<()> {
        assert_eq!(Pathspec::find_prefix_end(r"\*"), None);
        assert_eq!(Pathspec::find_prefix_end(r"*"), Some(0));
        assert_eq!(Pathspec::find_prefix_end(r"abc?"), Some(3));
        Ok(())
    }

    // TODO temporary
    #[test]
    pub fn test_pathspec_dot_matches_all() -> BitResult<()> {
        let pathspec = Pathspec::from_str(".")?;
        pathspec.matches_path("wer");
        pathspec.matches_path("foo/bar");
        Ok(())
    }

    #[test]
    pub fn pathspec_matches_test() -> BitResult<()> {
        let pathspec = Pathspec::from_str("hello")?;
        assert!(pathspec.matches_path("hello"));

        let pathspec = Pathspec::from_str("path")?;
        assert!(pathspec.matches_path("path/to/dir"));
        Ok(())
    }
}
