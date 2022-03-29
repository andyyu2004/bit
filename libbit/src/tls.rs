use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::repo::RepoCtxt;
use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    static REPO_CTXT: Cell<usize> = Cell::new(0);
}

pub(crate) fn enter_repo<R>(
    rcx: Arc<RepoCtxt>,
    f: impl FnOnce(BitRepo) -> BitResult<R>,
) -> BitResult<R> {
    REPO_CTXT.with(|ptr| {
        ptr.set(rcx as *const _ as usize);
        let r = with_repo(f);
        ptr.set(0);
        r
    })
}

pub(crate) fn with_repo<R>(f: impl FnOnce(BitRepo) -> R) -> R {
    let ctxt_ptr = REPO_CTXT.with(|ctxt| {
        debug_assert!(ctxt.get() != 0, "calling tls outside of repo context");
        ctxt.get()
    }) as *const RepoCtxt;
    let ctxt = unsafe { &*ctxt_ptr };
    ctxt.with(f)
}
