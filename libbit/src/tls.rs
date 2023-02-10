use crate::error::BitResult;
use crate::repo::BitRepo;
use crate::repo::RepoCtxt;
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    pub(crate) static REPO_CTXT: RefCell<Option<Arc<RepoCtxt>>> = Default::default();
}

pub(crate) fn enter_repo<R>(
    rcx: Arc<RepoCtxt>,
    f: impl FnOnce(BitRepo) -> BitResult<R>,
) -> BitResult<R> {
    REPO_CTXT.with(|ptr| {
        *ptr.borrow_mut() = Some(rcx);
        let r = with_repo(f);
        let rcx = ptr.borrow_mut().take().unwrap();
        assert_eq!(Arc::strong_count(&rcx), 1, "repo context leaked");
        r
    })
}

pub(crate) fn with_repo<R>(f: impl FnOnce(BitRepo) -> R) -> R {
    let rcx = REPO_CTXT.with(|ctxt| {
        Arc::clone(ctxt.borrow().as_ref().expect("calling tls outside of repo context"))
    });
    rcx.with(f)
}
