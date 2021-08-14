use crate::error::BitResult;
use crate::obj::{BitObjKind, Oid};
use rustc_hash::FxHashMap;

#[derive(Debug, Default)]
pub struct BitObjCache<'rcx> {
    // consider using LRU but is very unclear what size to use as most implementations require a fixed
    objects: FxHashMap<Oid, BitObjKind<'rcx>>,
}

impl<'rcx> BitObjCache<'rcx> {
    pub fn get_or_insert_with(
        &mut self,
        oid: Oid,
        f: impl FnOnce() -> BitResult<BitObjKind<'rcx>>,
    ) -> BitResult<BitObjKind<'rcx>> {
        if let Some(obj) = self.objects.get(&oid) {
            Ok(obj.clone())
        } else {
            let obj = f()?;
            self.objects.insert(oid, obj.clone());
            Ok(obj)
        }
    }
}
