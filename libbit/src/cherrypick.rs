use crate::error::BitResult;
use crate::obj::Oid;
use crate::peel::Peel;
use crate::refs::BitRef;
use crate::repo::BitRepo;

impl BitRepo {
    pub fn cherrypick_many(self, refs: impl IntoIterator<Item = BitRef>) -> BitResult<()> {
        for r in refs {
            self.cherrypick(r)?;
        }
        todo!("handle conflicts in loop above");
    }

    pub fn cherrypick_commit(self, oid: Oid) -> BitResult<()> {
        self.cherrypick(BitRef::Direct(oid))
    }

    pub fn cherrypick(self, reference: BitRef) -> BitResult<()> {
        let oid = self.fully_resolve_ref(reference)?;
        let commit = self.read_obj_commit(oid)?;
        ensure!(commit.parents.len() < 2, "TODO cherrypick merge commit");
        let cherrypick_parent = commit.parents.get(0).copied();
        let base = cherrypick_parent.map(|oid| oid.peel(self)).transpose()?;
        self.merge_with_base(reference, base, Default::default())?;
        // handle merge result
        Ok(())
    }
}
