use crate::error::BitResult;
use crate::obj::{BitObjKind, Oid};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct BitObjCache<'rcx> {
    // consider using LRU but is very unclear what size to use as most implementations require a fixed
    objects: FxHashMap<Oid, BitObjKind<'rcx>>,
}

impl<'rcx> BitObjCache<'rcx> {
    pub(crate) fn get_or_insert_with(
        &mut self,
        oid: Oid,
        f: impl FnOnce() -> BitResult<BitObjKind<'rcx>>,
    ) -> BitResult<BitObjKind<'rcx>> {
        if let Some(&obj) = self.objects.get(&oid) {
            Ok(obj)
        } else {
            let obj = f()?;
            self.objects.insert(oid, obj);
            Ok(obj)
        }
    }
}
