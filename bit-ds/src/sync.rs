use std::ops::{Deref, DerefMut};
use std::thread::ThreadId;

#[derive(Debug)]
pub struct OneThread<T> {
    thread: ThreadId,
    inner: T,
}

// SAFETY we check that the inner value is always accessed from the same thread
unsafe impl<T> std::marker::Sync for OneThread<T> {
}
unsafe impl<T> std::marker::Send for OneThread<T> {
}

impl<T> OneThread<T> {
    #[inline(always)]
    fn check(&self) {
        assert_eq!(std::thread::current().id(), self.thread);
    }

    #[inline(always)]
    pub fn new(inner: T) -> Self {
        OneThread { thread: std::thread::current().id(), inner }
    }

    #[inline(always)]
    pub fn into_inner(value: Self) -> T {
        value.check();
        value.inner
    }
}

impl<T> Deref for OneThread<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.check();
        &self.inner
    }
}

impl<T> DerefMut for OneThread<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.check();
        &mut self.inner
    }
}
