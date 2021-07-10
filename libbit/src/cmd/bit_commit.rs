use crate::error::BitResult;
use crate::obj::Oid;
use crate::pathspec::Pathspec;
use crate::refs::{BitRef, RefUpdateCause, RefUpdateCommitKind, SymbolicRef};
use crate::repo::BitRepo;
use enumflags2::bitflags;

#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
enum BitStatusFlags {
    Initial,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    // TODO return a BitCommitReport which includes the oid, and kind (CommitKind) etc
    pub fn commit(&self, msg: Option<String>) -> BitResult<Oid> {
        let head = self.read_head()?;
        let sym = match head {
            BitRef::Direct(..) => SymbolicRef::HEAD,
            BitRef::Symbolic(sym) => sym,
        };
        let parent = self.try_fully_resolve_ref(sym)?;

        let tree = self.write_tree()?;
        let head_tree = self.head_tree()?;

        // don't allow empty commits; also don't currently provide the option to do so as it's not that useful
        // the rhs of the disjunction checks for the case of an empty initial commit
        if tree == head_tree || head_tree.is_unknown() && tree == Oid::EMPTY_TREE {
            // rather oddly, we bail with the status report as the error message
            bail!(self.status(Pathspec::MATCH_ALL)?)
        }

        let oid = self.commit_tree(parent, msg, tree)?;
        let commit = self.read_obj(oid)?.into_commit();

        // TODO print status of commit
        // include initial commit if it is one
        // probably amend too (check with git)
        let cause = RefUpdateCause::Commit {
            subject: commit.message.subject.to_owned(),
            kind: if head_tree.is_known() {
                RefUpdateCommitKind::Normal
            } else {
                RefUpdateCommitKind::Initial
            },
        };

        self.update_ref(sym, oid, cause)?;
        Ok(oid)
    }
}
