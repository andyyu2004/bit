use crate::error::BitResult;
use crate::obj::{Commit, CommitMessage, Oid};
use crate::repo::BitRepo;
use std::str::FromStr;

impl<'r> BitRepo<'r> {
    pub fn bit_commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<()> {
        let (hash, _) = self.commit_tree(parent, message, tree)?;
        println!("{}", hash);
        Ok(())
    }

    pub fn commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<(Oid, Commit)> {
        let message = match &message {
            Some(msg) => CommitMessage::from_str(msg),
            None => self.read_commit_msg(),
        }?;
        let commit = self.mk_commit(tree, message, parent)?;
        let hash = self.write_obj(&commit)?;
        Ok((hash, commit))
    }
}
