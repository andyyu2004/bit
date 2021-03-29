use crate::error::BitGenericError;
use crate::index::BitIndexEntry;
use crate::obj::Tree;
use crate::path::BitPath;
use crate::tls;
use itertools::Itertools;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct Pathspec {
    /// non-wildcard prefix
    /// up to either the first wildcard or the last slash
    prefix: BitPath,
    // pathspec: Vec<()>,
}

pub struct PathspecMatches {}

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

    fn match_worktree(&self) -> PathspecMatches {
        tls::REPO.with(|repo| self.match_iterator(repo.worktree_iter()))
    }

    fn match_index(&self) -> PathspecMatches {
        tls::with_index(|index| self.match_iterator(index.iter()))
    }

    fn match_tree(&self, tree: &Tree) -> PathspecMatches {
        todo!()
    }

    // braindead implementation for now
    fn matches_path(&self, path: BitPath) -> bool {
        path.starts_with(self.prefix)
    }

    fn match_iterator(&self, iterator: impl Iterator<Item = BitIndexEntry>) -> PathspecMatches {
        PathspecMatches {}
    }

    fn is_wildcard(c: char) -> bool {
        return c == '*' || c == '?' || c == '[';
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
            None => &s[..],
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
