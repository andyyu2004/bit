use crate::error::BitResult;
use crate::obj::{CommitMessage, CommitParents, Oid};
use crate::repo::BitRepo;
use std::str::FromStr;

impl BitRepo {
    pub fn bit_commit_tree(
        &self,
        tree: Oid,
        parents: CommitParents,
        message: Option<String>,
    ) -> BitResult<()> {
        let oid = self.commit_tree(tree, parents, message)?;
        println!("{}", oid);
        Ok(())
    }

    pub fn commit_tree(
        &self,
        tree: Oid,
        parents: CommitParents,
        message: Option<String>,
    ) -> BitResult<Oid> {
        // arguably the act of calling into the editor should move out of lib into bin
        let message = match &message {
            Some(msg) => CommitMessage::from_str(msg),
            None => self.read_commit_msg(),
        }?;

        self.write_commit(tree, parents, message)
    }
}
