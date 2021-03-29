use crate::error::BitResult;
use crate::index::BitIndex;
use crate::repo::BitRepo;

scoped_thread_local!(pub static REPO: BitRepo);

pub(crate) fn with_repo<R>(repo: &BitRepo, f: impl FnOnce(&BitRepo) -> R) -> R {
    REPO.set(&repo, || REPO.with(f))
}

/// convenience functions to access the index without having a localrepo variable handy
pub(crate) fn with_index<R>(f: impl FnOnce(&BitIndex) -> R) -> R {
    REPO.with(|repo| repo.with_index(f))
}

pub(crate) fn with_index_mut<R>(f: impl FnOnce(&mut BitIndex) -> BitResult<R>) -> BitResult<R> {
    REPO.with(|repo| repo.with_index_mut(f))
}
