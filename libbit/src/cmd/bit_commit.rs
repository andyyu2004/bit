use crate::error::BitResult;
use crate::obj::Oid;
use crate::refs::BitRef;
use crate::refs::RefUpdateCause;
use crate::repo::BitRepo;

impl<'r> BitRepo<'r> {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    pub fn commit(&self, msg: Option<String>) -> BitResult<Oid> {
        let head = self.read_head()?;
        let sym = match head {
            BitRef::Direct(_oid) =>
                todo!("todo head is pointing to a commit not a branch (detached head state)"),
            BitRef::Symbolic(sym) => sym,
        };
        let parent = self.try_fully_resolve_ref(sym)?;
        let tree = self.write_tree()?;
        let (oid, commit) = self.commit_tree(parent, msg, tree)?;
        // does commit tree move head?
        // should the log on the current branches log or HEAD?
        // intuitively it makes sense to log on the branch as HEAD isn't actually moving
        // but the reflog doesn't contain any symbolic refs and only contains oids so
        // in that sense it is actually moving?
        // TODO check git's behaviour
        self.update_ref(sym, oid, RefUpdateCause::Commit { msg: commit.message, amend: false })?;
        Ok(oid)
    }
}
