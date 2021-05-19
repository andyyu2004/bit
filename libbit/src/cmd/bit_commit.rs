use crate::error::BitResult;
use crate::obj::Oid;
use crate::refs::BitRef;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn bit_commit(&self, message: Option<String>) -> BitResult<()> {
        let bitref = self.commit(message)?;
        println!("committed {}", bitref);
        Ok(())
    }

    pub fn commit(&self, message: Option<String>) -> BitResult<Oid> {
        let head = self.read_head()?;
        let sym = match head {
            BitRef::Direct(_oid) =>
                todo!("todo head is pointing to a commit not a branch (detached head state)"),
            BitRef::Symbolic(sym) => sym,
        };
        let parent = self.try_fully_resolve_ref(sym)?;
        let tree = self.write_tree()?;
        let oid = self.commit_tree(parent, message, tree)?;
        self.update_ref(sym, oid)?;
        Ok(oid)
    }
}
