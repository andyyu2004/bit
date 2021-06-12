#[macro_export]
macro_rules! dbg_entry_iter {
    ($iter:expr) => {{
        #[allow(unused)]
        use crate::iter::*;
        #[allow(unused)]
        use crate::obj::*;
        let iter = $iter
            .map(|entry| Ok(format!("{}:{} ({})", entry.path(), entry.mode(), entry.oid())))
            .collect::<Vec<_>>()?;
        dbg!(iter);
    }};
}
