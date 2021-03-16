use crate::path::BitPath;
use bumpalo::Bump as Arena;
use itertools::Itertools;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub(crate) struct PathInterner {
    arena: Arena,
    map: HashMap<&'static str, BitPath>,
    paths: Vec<&'static str>,
    components: HashMap<BitPath, &'static [&'static str]>,
}

impl PathInterner {
    pub fn intern(&mut self, s: &str) -> BitPath {
        if let Some(&x) = self.map.get(s) {
            return x;
        }
        let bitpath = BitPath::new(self.paths.len() as u32);
        // SAFETY we know this is valid utf8 as we just had a string
        let ptr: &str =
            unsafe { std::str::from_utf8_unchecked(self.arena.alloc_slice_copy(s.as_bytes())) };
        // SAFETY it is safe to cast to &'static as we will only access it while the arena is alive
        let static_str = unsafe { &*(ptr as *const str) };
        self.map.insert(static_str, bitpath);
        self.paths.push(static_str);
        bitpath
    }

    fn intern_components(&mut self, path: impl AsRef<Path>) -> &'static [&'static str] {
        // recursively interns each of the paths components
        let vec = path
            .as_ref()
            .iter()
            .map(|os_str| {
                let bitpath = self.intern(os_str.to_str().unwrap());
                self.get_str(bitpath)
            })
            .collect_vec();
        let slice = self.arena.alloc_slice_copy(&vec);
        // SAFETY &[&u8] to &[&str] where &str is known to be valid utf8;
        // from_utf8_unchecked is just a transmute under the hood anyway
        unsafe { std::mem::transmute(slice) }
    }

    pub fn get_components(&mut self, bitpath: BitPath) -> &'static [&'static str] {
        if let Some(components) = self.components.get(&bitpath) {
            return components;
        }
        let path = self.get_str(bitpath);
        let components = self.intern_components(path);
        assert!(self.components.insert(bitpath, components).is_none());
        components
    }

    pub fn get_str(&self, path: BitPath) -> &'static str {
        self.paths[path.index() as usize]
    }
}

thread_local! {
    static INTERNER: RefCell<PathInterner> = Default::default();
}

pub(crate) fn with_path_interner<R>(f: impl FnOnce(&mut PathInterner) -> R) -> R {
    INTERNER.with(|interner| f(&mut *interner.borrow_mut()))
}
