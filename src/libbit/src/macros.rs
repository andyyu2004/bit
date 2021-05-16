macro_rules! ensure_eq {
    ($a:expr, $b:expr) => {
        ensure!($a == $b)
    };
    ($a:expr, $b:expr, $($arg:tt)*) => {
        ensure!($a == $b, $($arg)*)
    };
}

macro_rules! bug {
    ($($arg:tt)*) => {
        eprintln!("BUG!");
        unreachable!($($arg)*)
    };
}

macro_rules! symbolic {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::SymbolicRef::from_str($sym).unwrap()
    }};
}

macro_rules! symbolic_ref {
    ($sym:expr) => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::BitRef::Symbolic(symbolic!($sym))
    }};
}

macro_rules! HEAD {
    () => {{
        #[allow(unused_imports)]
        use std::str::FromStr;
        crate::refs::BitRef::Symbolic(crate::refs::SymbolicRef::from_str("HEAD").unwrap())
    }};
}
