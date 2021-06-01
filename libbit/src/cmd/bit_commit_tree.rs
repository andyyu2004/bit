use crate::error::BitResult;
use crate::obj::Commit;
use crate::obj::Oid;
use crate::repo::BitRepo;

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
        let message = match message {
            Some(msg) => msg,
            None => self.read_commit_msg()?,
        };
        let commit = self.mk_commit(tree, message, parent)?;
        let hash = self.write_obj(&commit)?;
        Ok((hash, commit))
    }
}
