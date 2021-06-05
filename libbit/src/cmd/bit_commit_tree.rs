use crate::error::BitResult;
use crate::obj::{BitObj, Commit, CommitMessage, Oid};
use crate::repo::BitRepo;
use std::str::FromStr;

impl<'r> BitRepo<'r> {
    pub fn bit_commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<()> {
        let commit = self.commit_tree(parent, message, tree)?;
        println!("{}", commit.oid());
        Ok(())
    }

    pub fn commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<Commit> {
        let message = match &message {
            Some(msg) => CommitMessage::from_str(msg),
            None => self.read_commit_msg(),
        }?;

        let commit = self.mk_commit(tree, message, parent)?;
        self.write_obj(&commit)?;
        Ok(commit)
    }
}
