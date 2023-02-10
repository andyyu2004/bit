use crate::error::BitResult;
use crate::obj::{BitObjKind, Oid};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct BitObjCache {
    // consider using LRU but is very unclear what size to use as most implementations require a fixed
    objects: FxHashMap<Oid, BitObjKind>,
}

impl BitObjCache {
    pub(crate) fn get_or_insert_with(
        &mut self,
        oid: Oid,
        f: impl FnOnce() -> BitResult<BitObjKind>,
    ) -> BitResult<BitObjKind> {
        if let Some(obj) = self.objects.get(&oid) {
            Ok(obj.clone())
        } else {
            let obj = f()?;
            self.objects.insert(oid, obj.clone());
            Ok(obj)
        }
    }
}
