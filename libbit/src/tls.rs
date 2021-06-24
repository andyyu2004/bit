use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::repo::RepoCtxt;
use std::cell::Cell;

thread_local! {
    static REPO_CTXT: Cell<usize> = Cell::new(0);
}

pub(crate) fn enter_repo<'rcx, R>(
    ctxt: &'rcx RepoCtxt<'rcx>,
    f: impl FnOnce(BitRepo<'rcx>) -> BitResult<R>,
) -> BitResult<R> {
    REPO_CTXT.with(|ptr| ptr.set(ctxt as *const _ as usize));
    with_repo(f)
}

// use this function access the repo if you are going to return a `Result`
// otherwise there is some trouble with type inference
pub(crate) fn with_repo_res<'rcx, R>(
    f: impl FnOnce(BitRepo<'rcx>) -> BitResult<R>,
) -> BitResult<R> {
    let ctxt_ptr = REPO_CTXT.with(|ctxt| ctxt.get()) as *const RepoCtxt<'rcx>;
    let ctxt = unsafe { &*ctxt_ptr };
    ctxt.with(f)
}

pub(crate) fn with_repo<'rcx, R>(f: impl FnOnce(BitRepo<'rcx>) -> R) -> R {
    let ctxt_ptr = REPO_CTXT.with(|ctxt| ctxt.get()) as *const RepoCtxt<'rcx>;
    let ctxt = unsafe { &*ctxt_ptr };
    ctxt.with(f)
}
