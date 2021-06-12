use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::fmt::{self, Display, Formatter};
use typed_arena::Arena as TypedArena;

#[derive(Debug, Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq)]
pub struct Sym(usize);

impl Display for Sym {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Sym {
    pub fn as_str(self) -> &'static str {
        with_interner(|interner| interner.get_str(self))
    }

    pub fn intern(s: &str) -> Sym {
        with_interner(|interner| interner.intern(s))
    }
}

thread_local! {
    pub static INTERNER: RefCell<Interner> = Default::default();
}

fn with_interner<R>(f: impl FnOnce(&mut Interner) -> R) -> R {
    INTERNER.with(|cell| f(&mut cell.borrow_mut()))
}

#[derive(Default)]
pub struct Interner {
    symbols: FxHashMap<&'static str, Sym>,
    strs: Vec<&'static str>,
    arena: TypedArena<String>,
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(&sym) = self.symbols.get(s) {
            return sym;
        }
        let s: &str = &*self.arena.alloc(s.to_owned());
        // SAFETY: will only access strings while interner/arena is alive
        let s: &'static str = unsafe { &*(s as *const str) };
        let symbol = Sym(self.strs.len());
        self.strs.push(s);
        self.symbols.insert(s, symbol);
        symbol
    }

    pub fn get_str(&self, symbol: Sym) -> &'static str {
        self.strs[symbol.0]
    }
}
