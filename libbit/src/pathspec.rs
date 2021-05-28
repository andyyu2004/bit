use crate::error::{BitGenericError, BitResult};
use crate::index::BitIndex;
use crate::iter::BitEntryIterator;
use crate::path::BitPath;
use crate::repo::BitRepo;
use itertools::Itertools;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::str::FromStr;

// pathspec needs to be copy/static due to some lifetimes below
// or at least its much more convenient this way
// match_iterator is difficult otherwise
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Pathspec {
    /// non-wildcard prefix
    /// up to either the first wildcard or the last slash
    pub prefix: BitPath,
    // pathspec: Vec<()>,
}

impl Pathspec {
    pub fn new(prefix: BitPath) -> Self {
        Self { prefix }
    }

    // create a pathspec that matches anything
    // TODO consider making this a constant if possible
    pub fn match_all() -> Self {
        Self::new(BitPath::empty())
    }
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

impl BitRepo {
    pub fn match_worktree_with(
        &self,
        pathspec: &Pathspec,
    ) -> BitResult<impl BitEntryIterator + '_> {
        pathspec.match_worktree(self)
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

    pub fn match_worktree<'r>(self, repo: &'r BitRepo) -> BitResult<impl BitEntryIterator + 'r> {
        self.match_iterator(repo.worktree_iter()?)
    }

    pub fn match_index(self, index: &BitIndex<'_>) -> BitResult<impl BitEntryIterator> {
        self.match_iterator(index.iter())
    }

    pub fn match_head<'r>(&self, repo: &'r BitRepo) -> BitResult<impl BitEntryIterator + 'r> {
        self.match_iterator(repo.head_iter()?)
    }

    // braindead implementation for now
    pub fn matches_path(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref().starts_with(self.prefix)
    }

    fn match_iterator(self, iterator: impl BitEntryIterator) -> BitResult<impl BitEntryIterator> {
        Ok(iterator.filter(move |entry| Ok(self.matches_path(entry.path))))
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
            return Ok(Self::match_all());
        }
        let prefix_end = Self::find_prefix_end(&s);
        let prefix = match prefix_end {
            Some(i) => &s[..i],
            None => s,
        };
        Ok(Self::new(BitPath::intern(prefix)))
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
