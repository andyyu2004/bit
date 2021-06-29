use crate::path::BitPath;
use bumpalo::Bump as Arena;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::path::Path;

#[derive(Default)]
pub(crate) struct Interner {
    arena: Arena,
    map: FxHashMap<&'static str, BitPath>,
    set: FxHashSet<&'static str>,
    paths: Vec<&'static str>,
    components: FxHashMap<BitPath, &'static [BitPath]>,
}

pub trait Intern {
    fn intern(&self) -> &'static Self;
}

impl Intern for str {
    fn intern(&self) -> &'static Self {
        with_path_interner_mut(|interner| interner.intern_str(self))
    }
}

impl Interner {
    pub fn prefill(init: &[&'static str]) -> Self {
        Self {
            #[cfg(not(debug_assertions))]
            map: init.iter().copied().zip((0..).map(BitPath::new)).collect(),
            #[cfg(debug_assertions)]
            map: init
                .iter()
                .copied()
                .enumerate()
                .map(|(i, x)| (x, BitPath::new(i as u32, x)))
                .collect(),
            paths: init.to_vec(),
            ..Default::default()
        }
    }

    // this only exists due to some lifetime difficulties with the GitConfig parser
    pub fn intern_str(&mut self, s: &str) -> &'static str {
        // could potentially reuse same allocation as some path, but its really insignificant
        if let Some(&x) = self.set.get(s) {
            return x;
        }

        let ptr: &str =
            unsafe { std::str::from_utf8_unchecked(self.arena.alloc_slice_copy(s.as_bytes())) };
        let static_str = unsafe { &*(ptr as *const str) };
        self.set.insert(static_str);
        static_str
    }

    pub fn intern_path(&mut self, s: &str) -> BitPath {
        if let Some(&x) = self.map.get(s) {
            return x;
        }
        // SAFETY we know this is valid utf8 as we just had a string
        let ptr: &str =
            unsafe { std::str::from_utf8_unchecked(self.arena.alloc_slice_copy(s.as_bytes())) };
        // SAFETY it is safe to cast to &'static as we will only access it while the arena is alive
        let static_str = unsafe { &*(ptr as *const str) };
        let bitpath = BitPath::new(
            self.paths.len() as u32,
            #[cfg(debug_assertions)]
            static_str,
        );
        self.map.insert(static_str, bitpath);
        self.paths.push(static_str);

        debug_assert_eq!(self.intern_path(s), bitpath);
        debug_assert_eq!(self.get_str(bitpath), s);

        bitpath
    }

    fn intern_components(&mut self, path: impl AsRef<Path>) -> &'static [BitPath] {
        // recursively interns each of the paths components
        let vec = path
            .as_ref()
            .iter()
            .map(|os_str| self.intern_path(os_str.to_str().unwrap()))
            .collect_vec();
        unsafe { &*(self.arena.alloc_slice_copy(&vec) as *const _) }
    }

    pub fn get_components(&mut self, bitpath: BitPath) -> &'static [BitPath] {
        if let Some(components) = self.components.get(&bitpath) {
            return components;
        }
        let path = self.get_str(bitpath);
        let components = self.intern_components(path);
        debug_assert!(self.components.insert(bitpath, components).is_none());
        components
    }

    pub fn get_str(&self, path: BitPath) -> &'static str {
        self.paths[path.index() as usize]
    }
}

macro_rules! prefill {
    (@index $idx:expr) => {};
    (@index $idx:expr, $name:ident => $lit:literal) => {{
        impl BitPath {
            pub const $name: Self = Self::new($idx, #[cfg(debug_assertions)] $lit);
        }
    }};
    (@index $idx:expr, $name:ident => $lit:literal, $($tail:tt)*) => {{
        // items are statements so we can sort of "abuse" the rust grammar a bit here
        // and put the BitPath impl in statement position

        impl BitPath {
            pub const $name: Self = Self::new($idx, #[cfg(debug_assertions)] $lit);
        }

        prefill!(@index 1u32 + $idx, $($tail)*)
    }};
    ($($name:ident => $lit:literal),*) => {{
        prefill!(@index 0u32, $($name => $lit),*);
        &[$($lit),*]
    }}
}

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::prefill(prefill! {
        EMPTY => "",
        HEAD => "HEAD",
        DOT_GIT => ".git",
        A => "a",
        B => "b"
    }));
}

pub(crate) fn with_path_interner<R>(f: impl FnOnce(&Interner) -> R) -> R {
    INTERNER.with(|interner| f(&*interner.borrow()))
}

pub(crate) fn with_path_interner_mut<R>(f: impl FnOnce(&mut Interner) -> R) -> R {
    INTERNER.with(|interner| f(&mut *interner.borrow_mut()))
}
