use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::repo::RepoCtxt;
use std::cell::Cell;

thread_local! {
    static REPO_CTXT: Cell<usize> = Cell::new(0);
}

pub(crate) fn enter_repo<'r, R>(
    ctxt: &'r RepoCtxt<'r>,
    f: impl FnOnce(BitRepo<'r>) -> BitResult<R>,
) -> BitResult<R> {
    REPO_CTXT.with(|ptr| ptr.set(ctxt as *const _ as usize));
    with_repo(f)
}

// use this function access the repo if you are going to return a `Result`
// otherwise there is some trouble with type inference
pub(crate) fn with_repo_res<'r, R>(f: impl FnOnce(BitRepo<'r>) -> BitResult<R>) -> BitResult<R> {
    let ctxt_ptr = REPO_CTXT.with(|ctxt| ctxt.get()) as *const RepoCtxt<'r>;
    let ctxt = unsafe { &*ctxt_ptr };
    ctxt.with(f)
}

pub(crate) fn with_repo<'r, R>(f: impl FnOnce(BitRepo<'r>) -> R) -> R {
    let ctxt_ptr = REPO_CTXT.with(|ctxt| ctxt.get()) as *const RepoCtxt<'r>;
    let ctxt = unsafe { &*ctxt_ptr };
    ctxt.with(f)
}
