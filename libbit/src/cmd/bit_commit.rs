use crate::error::BitResult;
use crate::obj::Oid;
use crate::refs::{BitRef, RefUpdateCause, RefUpdateCommitKind};
use crate::repo::BitRepo;

impl<'r> BitRepo<'r> {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    // TODO return a BitCommitReport which includes the oid, and kind (CommitKind) etc
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

        // TODO print status of commit
        // include initial commit if it is one
        // probably amend too (check with git)
        let cause = RefUpdateCause::Commit {
            subject: commit.message.subject,
            kind: if head_tree.is_known() {
                RefUpdateCommitKind::Normal
            } else {
                RefUpdateCommitKind::Initial
            },
        };

        self.update_ref(sym, oid, cause.clone())?;
        Ok(oid)
    }
}
