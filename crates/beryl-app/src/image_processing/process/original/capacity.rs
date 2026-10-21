use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use super::*;

pub(crate) struct OriginalCapacityChanged {
    event: Arc<OriginalCapacityEvent>,
    waiter: Arc<CapacityWaiter>,
    revision: u64,
    window: Option<Weak<WindowState>>,
}

pub(in super::super) struct OriginalCapacityEvent {
    bound: usize,
    state: Mutex<CapacityState>,
}

#[derive(Default)]
struct CapacityState {
    revision: u64,
    exhausted: bool,
    waiters: Vec<Weak<CapacityWaiter>>,
}

#[derive(Default)]
struct CapacityWaiter {
    waker: Mutex<Option<Waker>>,
}

impl OriginalCapacityEvent {
    pub(in super::super) fn new(limits: MediaResourceLimits) -> Self {
        Self {
            bound: beryl_state::MAX_RESTORABLE_WINDOWS
                .checked_mul(limits.configuration().window_pin_entries)
                .expect("validated capacity waiter bound"),
            state: Mutex::new(CapacityState::default()),
        }
    }

    pub(super) fn changed(
        self: &Arc<Self>,
        window: Weak<WindowState>,
    ) -> Result<OriginalCapacityChanged, ImageMediaError> {
        self.subscribe(Some(window))
    }

    #[cfg(test)]
    pub(super) fn unbound_changed(
        self: &Arc<Self>,
    ) -> Result<OriginalCapacityChanged, ImageMediaError> {
        self.subscribe(None)
    }

    fn subscribe(
        self: &Arc<Self>,
        window: Option<Weak<WindowState>>,
    ) -> Result<OriginalCapacityChanged, ImageMediaError> {
        let mut state = lock(&self.state);
        state.waiters.retain(|waiter| waiter.strong_count() != 0);
        if state.exhausted || state.waiters.len() >= self.bound {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let waiter = Arc::new(CapacityWaiter::default());
        state.waiters.push(Arc::downgrade(&waiter));
        Ok(OriginalCapacityChanged {
            event: self.clone(),
            waiter,
            revision: state.revision,
            window,
        })
    }

    pub(in super::super) fn notify(&self) {
        let waiters = {
            let mut state = lock(&self.state);
            if let Some(revision) = state.revision.checked_add(1) {
                state.revision = revision;
            } else {
                state.exhausted = true;
            }
            state.waiters.retain(|waiter| waiter.strong_count() != 0);
            state
                .waiters
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>()
        };
        for waiter in waiters {
            let waker = lock(&waiter.waker).take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

impl Future for OriginalCapacityChanged {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let state = lock(&self.event.state);
        let retired = self.window.as_ref().is_some_and(|window| {
            !window.upgrade().is_some_and(|window| {
                !window.closed.load(Ordering::Acquire)
                    && window
                        .attachment
                        .upgrade()
                        .is_some_and(|attachment| !attachment.retired.load(Ordering::Acquire))
            })
        });
        if state.exhausted || state.revision != self.revision || retired {
            return Poll::Ready(());
        }
        *lock(&self.waiter.waker) = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for OriginalCapacityChanged {
    fn drop(&mut self) {
        lock(&self.event.state).waiters.retain(|waiter| {
            !Weak::ptr_eq(waiter, &Arc::downgrade(&self.waiter)) && waiter.strong_count() != 0
        });
        lock(&self.waiter.waker).take();
    }
}
