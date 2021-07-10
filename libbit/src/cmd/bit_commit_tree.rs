use crate::error::BitResult;
use crate::obj::{CommitMessage, Oid};
use crate::repo::BitRepo;
use std::str::FromStr;

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<()> {
        let oid = self.commit_tree(parent, message, tree)?;
        println!("{}", oid);
        Ok(())
    }

    pub fn commit_tree(
        &self,
        parent: Option<Oid>,
        message: Option<String>,
        tree: Oid,
    ) -> BitResult<Oid> {
        // arguably the act of calling into the editor should move out of lib into bin
        let message = match &message {
            Some(msg) => CommitMessage::from_str(msg),
            None => self.read_commit_msg(),
        }?;

        self.mk_commit(tree, message, parent)
    }
}
