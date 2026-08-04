use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
pub(super) struct SourcePageHandoffBarriers {
    installed: Mutex<Option<Arc<SourcePageHandoffBarrier>>>,
}

#[derive(Debug)]
struct SourcePageHandoffBarrier {
    target_request: usize,
    state: Mutex<BarrierState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct BarrierState {
    paused: bool,
    released: bool,
}

/// Controls one request-local pause at the input-replay page broker handoff.
#[doc(hidden)]
#[derive(Debug)]
pub struct SourcePageHandoffBarrierController {
    barrier: Arc<SourcePageHandoffBarrier>,
}

impl SourcePageHandoffBarriers {
    pub(super) fn install(&self, target_request: usize) -> SourcePageHandoffBarrierController {
        assert!(
            target_request != 0,
            "source-page barrier target is one-based"
        );
        let barrier = Arc::new(SourcePageHandoffBarrier {
            target_request,
            state: Mutex::new(BarrierState::default()),
            changed: Condvar::new(),
        });
        let mut installed = self
            .installed
            .lock()
            .expect("source-page handoff barrier registry is usable");
        assert!(
            installed.is_none(),
            "ordinary-input diagnostics already own a source-page handoff barrier"
        );
        *installed = Some(Arc::clone(&barrier));
        SourcePageHandoffBarrierController { barrier }
    }

    pub(super) fn pause_if_target(&self, request: usize) {
        let barrier = self
            .installed
            .lock()
            .expect("source-page handoff barrier registry is usable")
            .clone();
        let Some(barrier) = barrier.filter(|barrier| barrier.target_request == request) else {
            return;
        };
        let mut state = barrier
            .state
            .lock()
            .expect("source-page handoff barrier is usable");
        if state.released {
            return;
        }
        state.paused = true;
        barrier.changed.notify_all();
        while !state.released {
            state = barrier
                .changed
                .wait(state)
                .expect("source-page handoff barrier remains usable");
        }
    }
}

impl SourcePageHandoffBarrierController {
    /// Waits until the selected source-page request is held before its broker reply.
    #[must_use]
    pub fn wait_until_paused(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .barrier
            .state
            .lock()
            .expect("source-page handoff barrier is usable");
        while !state.paused && !state.released {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next, timed) = self
                .barrier
                .changed
                .wait_timeout(state, remaining)
                .expect("source-page handoff barrier remains usable");
            state = next;
            if timed.timed_out() {
                break;
            }
        }
        state.paused
    }

    /// Releases the held page to the capacity-one broker reply.
    pub fn release(&self) {
        let mut state = self
            .barrier
            .state
            .lock()
            .expect("source-page handoff barrier is usable");
        state.released = true;
        self.barrier.changed.notify_all();
    }
}

impl Drop for SourcePageHandoffBarrierController {
    fn drop(&mut self) {
        self.release();
    }
}
