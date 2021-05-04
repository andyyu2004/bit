macro_rules! ensure_eq {
    ($a:expr, $b:expr) => {
        ensure!($a == $b)
    };
    ($a:expr, $b:expr, $($arg:tt)*) => {
        ensure!($a == $b, $($arg)*)
    };
}
