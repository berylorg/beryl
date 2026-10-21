use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};

use beryl_home_store::CommandCancellation;

#[derive(Default)]
pub(super) struct NativeProgress {
    revision: AtomicU64,
    exhausted: AtomicBool,
}

impl NativeProgress {
    #[cfg(test)]
    pub(super) fn at_revision_for_tests(revision: u64) -> Self {
        Self {
            revision: AtomicU64::new(revision),
            exhausted: AtomicBool::new(false),
        }
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
    pub(super) fn is_exhausted(&self) -> bool {
        self.exhausted.load(Ordering::Acquire)
    }
    pub(super) fn advance(&self) -> Result<(), super::ImageMediaError> {
        if self.is_exhausted()
            || self
                .revision
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |revision| {
                    revision.checked_add(1)
                })
                .is_err()
        {
            self.exhausted.store(true, Ordering::Release);
            return Err(super::ImageMediaError(super::ImageMediaFailure::Capacity));
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct ChangeEvent {
    revision: AtomicU64,
    waker: Mutex<Option<Waker>>,
}

pub(crate) struct ImageMediaChanged {
    event: Arc<ChangeEvent>,
    revision: u64,
    ready: bool,
    native_retry: Option<NativeRetry>,
}

struct NativeRetry {
    progress: Arc<NativeProgress>,
    requested: u64,
    cancellation: CommandCancellation,
}

impl NativeRetry {
    fn ready(&self) -> bool {
        self.cancellation.is_cancelled()
            || self.progress.is_exhausted()
            || self.progress.revision() != self.requested
    }
}

impl ChangeEvent {
    pub(super) fn changed(self: &Arc<Self>, ready: bool) -> ImageMediaChanged {
        ImageMediaChanged {
            event: self.clone(),
            revision: self.revision.load(Ordering::Acquire),
            ready,
            native_retry: None,
        }
    }
    pub(super) fn native_retry_changed(
        self: &Arc<Self>,
        progress: Arc<NativeProgress>,
        requested: u64,
        cancellation: CommandCancellation,
    ) -> ImageMediaChanged {
        ImageMediaChanged {
            event: self.clone(),
            revision: self.revision.load(Ordering::Acquire),
            ready: false,
            native_retry: Some(NativeRetry {
                progress,
                requested,
                cancellation,
            }),
        }
    }
    pub(super) fn notify(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        let waker = super::lock(&self.waker).take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for ImageMediaChanged {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(retry) = &self.native_retry {
            if retry.ready() {
                return Poll::Ready(());
            }
            *super::lock(&self.event.waker) = Some(cx.waker().clone());
            return if retry.ready() {
                Poll::Ready(())
            } else {
                Poll::Pending
            };
        }
        if self.ready || self.event.revision.load(Ordering::Acquire) != self.revision {
            return Poll::Ready(());
        }
        *super::lock(&self.event.waker) = Some(cx.waker().clone());
        if self.event.revision.load(Ordering::Acquire) != self.revision {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
