use crate::hash::MakeHash;
use crate::path::BitPath;
use bumpalo::Bump as Arena;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::RawEntryMut;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

// This interner deals only with `OsStr` (instead of `Path`) to avoid normalization issues.
// In particular, we want trailing slashes to be significant (it gets normalized away by path)
#[derive(Default)]
pub(crate) struct Interner {
    arena: Arena,
    map: FxHashMap<&'static OsStr, BitPath>,
    set: FxHashSet<&'static str>,
    // paths: Vec<&'static OsStr>,
    pathc: Cell<u32>,
    components: FxHashMap<BitPath, &'static [BitPath]>,
}

pub trait Intern {
    fn intern(&self) -> &'static Self;
}

impl Intern for str {
    fn intern(&self) -> &'static Self {
        with_path_interner(|interner| interner.intern_str(self))
    }
}

impl Interner {
    pub fn prefill(init: &[&'static OsStr]) -> Self {
        Self {
            map: init
                .iter()
                .copied()
                .enumerate()
                .map(|(i, os_str)| (os_str, BitPath::new(i as u32, os_str)))
                .collect(),
            pathc: Cell::new(init.len() as u32),
            // paths: init.iter().copied().collect(),
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

    pub fn intern_path(&mut self, path: impl AsRef<OsStr>) -> BitPath {
        let path = path.as_ref();
        let hash = path.mk_fx_hash();
        match self.map.raw_entry_mut().from_key_hashed_nocheck(hash, path) {
            RawEntryMut::Occupied(entry) => *entry.get(),
            RawEntryMut::Vacant(entry) => {
                let ptr = self.arena.alloc_slice_copy(path.as_bytes());
                // // SAFETY it is safe to cast to &'static as we will only access it while the arena contained in `self` is alive
                let static_path = OsStr::from_bytes(unsafe { &*(ptr as *const [u8]) });
                let next_idx = self.pathc.get();
                self.pathc.set(next_idx + 1);
                let bitpath = BitPath::new(next_idx, static_path);
                debug_assert_eq!(
                    static_path.mk_fx_hash(),
                    hash,
                    "hash of the interned path is different from the hash of the original path"
                );
                entry.insert_hashed_nocheck(hash, static_path, bitpath);
                bitpath
            }
        }
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
        // let path = self.get_path(bitpath);
        let path = bitpath.as_path();
        let components = self.intern_components(path);
        debug_assert!(self.components.insert(bitpath, components).is_none());
        components
    }
}

const fn str_as_os_str(s: &str) -> &OsStr {
    // SAFETY this is roughly the same implementation as `OsStr::from_bytes`
    // can't use transmute here as it's not usable in const fn currently
    unsafe { std::mem::transmute_copy(&s.as_bytes()) }
}

macro_rules! prefill {
    (@index $idx:expr) => {};
    (@index $idx:expr, $name:ident => $lit:literal) => {{
        impl BitPath {
            pub const $name: Self = Self::new($idx, str_as_os_str($lit));
        }
    }};
    (@index $idx:expr, $name:ident => $lit:literal, $($tail:tt)*) => {{
        // items are statements so we can sort of "abuse" the rust grammar a bit here
        // and put the BitPath impl in statement position

        impl BitPath {
            pub const $name: Self = Self::new($idx, str_as_os_str($lit));
        }

        prefill!(@index 1u32 + $idx, $($tail)*)
    }};
    ($($name:ident => $lit:literal),*) => {{
        prefill!(@index 0u32, $($name => $lit),*);
        &[$(str_as_os_str($lit)),*]
    }}
}

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::prefill(prefill! {
        EMPTY => "",
        HEAD => "HEAD",
        MASTER => "master",
        MERGE_HEAD => "MERGE_HEAD",
        DOT_GIT => ".git",
        DOT_BIT => ".bit",
        REMOVED => "removed",
        REFS_HEADS => "refs/heads",
        ATSYM => "@",
        REFS_TAGS => "refs/tags",
        REFS_REMOTES => "refs/remotes",
        A => "a",
        B => "b"
    }));
}

pub(crate) fn with_path_interner<R>(f: impl FnOnce(&mut Interner) -> R) -> R {
    INTERNER.with(|interner| f(&mut *interner.borrow_mut()))
}
