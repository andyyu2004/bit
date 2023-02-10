#[macro_export]
macro_rules! dbg_entry_iter {
    ($iter:expr) => {{
        use $crate::iter::*;
        use $crate::obj::*;
        let iter = $iter
            .map(|entry| Ok(format!("{}:{} ({})", entry.path(), entry.mode(), entry.oid())))
            .collect::<Vec<_>>()?;
        dbg!(iter);
    }};
}
