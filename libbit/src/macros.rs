macro_rules! ensure_eq {
    ($a:expr, $b:expr) => {
        ensure!($a == $b)
    };
    ($a:expr, $b:expr, $($arg:tt)*) => {
        ensure!($a == $b, $($arg)*)
    };
}

macro_rules! arrayvec {
    ($($tt:tt)*) => { arrayvec::ArrayVec::from([$($tt)*])}
}

macro_rules! bug {
    ($($arg:tt)*) => {{
        eprintln!("BUG! Please file an issue!");
        panic!($($arg)*)
    }};
}

macro_rules! pluralize {
    ($count:expr) => {
        if $count == 1 { "" } else { "s" }
    };
}
