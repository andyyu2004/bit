use crate::repo::BitRepo;

scoped_thread_local!(static REPO: BitRepo);

pub(crate) fn with<R>(repo: &BitRepo, f: impl FnOnce(&BitRepo) -> R) -> R {
    REPO.set(&repo, || REPO.with(f))
}
