use crate::error::BitGenericError;
use crate::index::BitIndexEntry;
use crate::path::BitPath;
use crate::tls;
use fallible_iterator::FallibleIterator;
use itertools::Itertools;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Pathspec {
    /// non-wildcard prefix
    /// up to either the first wildcard or the last slash
    pub prefix: BitPath,
    // pathspec: Vec<()>,
}

#[derive(Debug, Clone)]
pub struct PathspecMatch {
    index_entry: BitIndexEntry,
}

impl PathspecMatch {
    pub fn new(index_entry: BitIndexEntry) -> Self {
        Self { index_entry }
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

    pub fn match_worktree(
        self,
    ) -> impl FallibleIterator<Item = PathspecMatch, Error = BitGenericError> {
        tls::REPO.with(|repo| self.match_iterator(repo.worktree_iter()))
    }

    pub fn match_index(
        self,
    ) -> impl FallibleIterator<Item = PathspecMatch, Error = BitGenericError> {
        tls::with_index(|index| self.match_iterator(index.iter()))
    }

    // pub fn match_tree(
    //     &self,
    //     tree: &Tree,
    // ) -> impl FallibleIterator<Item = PathspecMatch, Error = BitGenericError> {
    // }

    // braindead implementation for now
    pub fn matches_path(&self, path: BitPath) -> bool {
        dbg!(self);
        dbg!(path);
        path.starts_with(self.prefix)
    }

    fn match_iterator(
        self,
        iterator: impl FallibleIterator<Item = BitIndexEntry, Error = BitGenericError>,
    ) -> impl FallibleIterator<Item = PathspecMatch, Error = BitGenericError> {
        iterator
            .filter(move |entry| Ok(self.matches_path(entry.filepath)))
            .map(|entry| Ok(PathspecMatch::new(entry)))
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

    #[test]
    pub fn pathspec_matches_test() -> BitResult<()> {
        let pathspec = Pathspec::from_str("hello")?;
        assert!(pathspec.matches_path("hello".into()));

        let pathspec = Pathspec::from_str("path")?;
        assert!(pathspec.matches_path("path/to/dir".into()));
        Ok(())
    }
}
