use crate::path::BitPath;
use bumpalo::Bump as Arena;
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

// This interner deals only with `OsStr` (instead of `Path`) to avoid normalization issues.
// In particular, we want trailing slashes to be significant (it gets normalized away by path)
#[derive(Default)]
pub(crate) struct Interner {
    arena: Arena,
    set: FxHashSet<&'static str>,
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
    pub fn prefill(_init: &[&'static OsStr]) -> Self {
        Self { ..Default::default() }
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
        let ptr = self.arena.alloc_slice_copy(path.as_ref().as_bytes());
        // // SAFETY it is safe to cast to &'static as we will only access it while the arena contained in `self` is alive
        let static_path = OsStr::from_bytes(unsafe { &*(ptr as *const [u8]) });
        BitPath::new(static_path)
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
            pub const $name: Self = Self::new(str_as_os_str($lit));
        }
    }};
    (@index $idx:expr, $name:ident => $lit:literal, $($tail:tt)*) => {{
        // items are statements so we can sort of "abuse" the rust grammar a bit here
        // and put the BitPath impl in statement position

        impl BitPath {
            pub const $name: Self = Self::new(str_as_os_str($lit));
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
        MERGE_HEAD => "MERGE_HEAD",
        DOT_GIT => ".git",
        DOT_BIT => ".bit",
        REMOVED => "removed",
        ATSYM => "@",
        REFS_HEADS => "refs/heads",
        REFS_TAGS => "refs/tags",
        REFS_REMOTES => "refs/remotes",
        REFS_STASH => "refs/stash",
        MASTER => "refs/heads/master",
        A => "a",
        B => "b"
    }));
}

pub(crate) fn with_path_interner<R>(f: impl FnOnce(&mut Interner) -> R) -> R {
    INTERNER.with(|interner| f(&mut interner.borrow_mut()))
}
