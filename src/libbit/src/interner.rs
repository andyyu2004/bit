use crate::path::BitPath;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct PathInterner {
    arena: typed_arena::Arena<String>,
    map: HashMap<&'static str, BitPath>,
    paths: Vec<&'static str>,
}

impl PathInterner {
    pub fn intern(&mut self, s: &str) -> BitPath {
        if let Some(&x) = self.map.get(s) {
            return x;
        }
        let path = BitPath::new(self.paths.len() as u32);
        let ptr: &str = self.arena.alloc(s.to_owned());
        // it is safe to cast to &'static as we will only access it while the arena is alive
        let static_str = unsafe { &*(ptr as *const str) };
        self.map.insert(static_str, path);
        self.paths.push(static_str);
        path
    }

    pub fn get_str(&self, path: BitPath) -> &'static str {
        self.paths[path.index() as usize]
    }
}

thread_local! {
    static INTERNER: RefCell<PathInterner> = Default::default();
}

pub(crate) fn with_interner<R>(f: impl FnOnce(&mut PathInterner) -> R) -> R {
    INTERNER.with(|interner| f(&mut *interner.borrow_mut()))
}
