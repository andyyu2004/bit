use crate::error::BitResult;
use crate::obj::{BitObjKind, BitRawObj, Oid, WritableObject};
use crate::repo::BitRepo;
use rustc_hash::FxHashMap;
use std::io::Cursor;

#[derive(Default)]
pub struct BitObjCache {
    // consider using LRU but is very unclear what size to use as most implementations require a fixed
    objects: FxHashMap<Oid, BitObjKind>,
}

impl BitObjCache {
    pub(crate) fn get(&self, oid: Oid) -> BitObjKind {
        self.objects[&oid]
    }

    pub(crate) fn insert(&mut self, oid: Oid, obj: BitObjKind) {
        self.objects.insert(oid, obj);
    }

    pub(crate) fn get_or_insert_with(
        &mut self,
        oid: Oid,
        f: impl FnOnce() -> BitResult<BitObjKind>,
    ) -> BitResult<BitObjKind> {
        if let Some(&obj) = self.objects.get(&oid) {
            Ok(obj)
        } else {
            let obj = f()?;
            self.objects.insert(oid, obj);
            Ok(obj)
        }
    }
}

/// A pseudo-odb backed directly by the object cache
pub(crate) struct VirtualOdb {
    repo: BitRepo,
}

impl VirtualOdb {
    pub fn new(repo: BitRepo) -> Self {
        Self { repo }
    }

    pub fn write(&self, obj: &dyn WritableObject) -> BitResult<Oid> {
        // a bit of a weird implementation of write,
        // writes out the object bytes and then parses it
        // probably a better way?
        let (oid, bytes) = obj.hash_and_serialize()?;
        let raw = BitRawObj::from_stream(oid, Box::new(Cursor::new(bytes)))?;
        let obj = BitObjKind::from_raw(self.repo, raw)?;
        self.repo.cache().write().insert(oid, obj);
        Ok(oid)
    }

    pub fn read(&self, oid: Oid) -> BitObjKind {
        self.repo.cache().read().get(oid)
    }
}
