use crate::error::BitResult;
use crate::obj::Oid;
use crate::refs::{BitRef, RefUpdateCause, RefUpdateCommitKind, SymbolicRef};
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
        let head_tree = self.head_tree_oid()?;

        // don't allow empty commits; also don't currently provide the option to do so as it's not that useful
        if tree == head_tree {
            bail!("nothing to commit");
        } else {
            // TODO initial commit check index entries is empty (or otherwise)?
            // TODO also check for untracked files and show those and suggest adding them
        }

        let (oid, commit) = self.commit_tree(parent, msg, tree)?;

        // TODO if head_tree.is_unknown() print (root-commit)
        // or return something that can be printed as a status
        // HEAD is already pointing at sym (as that's how we got the value of sym anyway)
        // we do this just to write something to HEAD's reflog
        // this must happen after updating the ref above
        if head_tree.is_unknown() {
            let cause = RefUpdateCause::Commit {
                subject: commit.message.subject,
                kind: RefUpdateCommitKind::Initial,
            };
            self.update_ref(sym, oid, cause.clone())?;
            self.update_ref(SymbolicRef::HEAD, sym, cause)?;
        } else {
            // does commit tree move head?
            // should the log on the current branches log or HEAD?
            // intuitively it makes sense to log on the branch as HEAD isn't actually moving
            // but the reflog doesn't contain any symbolic refs and only contains oids so
            // in that sense it is actually moving?
            // TODO check git's behaviour
            let cause = RefUpdateCause::Commit {
                subject: commit.message.subject,
                kind: RefUpdateCommitKind::Normal,
            };
            self.update_ref(sym, oid, cause)?;
        }
        Ok(oid)
    }
}
