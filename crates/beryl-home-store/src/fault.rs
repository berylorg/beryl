#[cfg(feature = "test-faults")]
use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FaultPoint {
    BeforeCommit,
    AfterCommitBeforePersist,
    AfterPersist,
    BeforeVerification,
    BeforeReopen,
    AfterReopen,
    BeforeSidecarDirectorySync,
    BeforeSidecarWrite,
    BeforeSidecarFileSync,
    BeforeSidecarRename,
    AfterSidecarRename,
    BeforeSidecarVerification,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Debug)]
enum FaultAction {
    Error(io::ErrorKind),
    Panic,
    Abort,
    Block(Arc<BlockState>),
}

#[cfg(feature = "test-faults")]
#[derive(Debug, Default)]
struct BlockState {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

/// Handle for one deterministic blocked concrete-boundary test cut point.
#[cfg(feature = "test-faults")]
#[derive(Clone, Debug)]
pub struct FaultBlock {
    state: Arc<BlockState>,
}

#[cfg(feature = "test-faults")]
impl FaultBlock {
    /// Waits until the owning store reaches this cut point.
    #[must_use]
    pub fn wait_until_reached(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .state
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !state.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, wait) = self
                .state
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            if wait.timed_out() && !state.0 {
                return false;
            }
        }
        true
    }

    /// Releases the blocked store operation.
    pub fn release(&self) {
        let mut state = self
            .state
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.1 = true;
        self.state.changed.notify_all();
    }
}

/// Store-local deterministic cut-point controller used only by fault tests.
#[cfg(feature = "test-faults")]
#[derive(Clone, Default)]
pub struct FaultController {
    actions: Arc<Mutex<HashMap<FaultPoint, VecDeque<FaultAction>>>>,
}

#[cfg(feature = "test-faults")]
impl FaultController {
    /// Creates an empty controller whose cut points all pass through.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next visit to one cut point return a synthetic I/O failure.
    pub fn fail_next(&self, point: FaultPoint) {
        self.fail_next_with_kind(point, io::ErrorKind::Other);
    }

    /// Makes the next visit return a synthetic I/O failure with an exact kind.
    pub fn fail_next_with_kind(&self, point: FaultPoint, kind: io::ErrorKind) {
        self.push(point, FaultAction::Error(kind));
    }

    /// Makes the next `count` visits return synthetic I/O failures.
    pub fn fail_times(&self, point: FaultPoint, count: usize) {
        self.fail_times_with_kind(point, io::ErrorKind::Other, count);
    }

    /// Makes the next `count` visits return failures with the exact I/O kind.
    pub fn fail_times_with_kind(&self, point: FaultPoint, kind: io::ErrorKind, count: usize) {
        for _ in 0..count {
            self.push(point, FaultAction::Error(kind));
        }
    }

    /// Makes the next visit panic on the thread executing the store operation.
    pub fn panic_next(&self, point: FaultPoint) {
        self.push(point, FaultAction::Panic);
    }

    /// Makes the next visit terminate its subprocess immediately.
    pub fn abort_next(&self, point: FaultPoint) {
        self.push(point, FaultAction::Abort);
    }

    /// Blocks the next visit until the returned handle releases it.
    #[must_use]
    pub fn block_next(&self, point: FaultPoint) -> FaultBlock {
        let state = Arc::new(BlockState::default());
        self.push(point, FaultAction::Block(state.clone()));
        FaultBlock { state }
    }

    fn push(&self, point: FaultPoint, action: FaultAction) {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(point)
            .or_default()
            .push_back(action);
    }

    pub(crate) fn check(&self, point: FaultPoint) -> io::Result<()> {
        let action = self
            .actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&point)
            .and_then(VecDeque::pop_front);
        match action {
            None => Ok(()),
            Some(FaultAction::Error(kind)) => Err(io::Error::new(
                kind,
                format!("synthetic Beryl-home fault at {point:?}"),
            )),
            Some(FaultAction::Panic) => {
                panic!("synthetic Beryl-home panic at {point:?}")
            }
            Some(FaultAction::Abort) => std::process::abort(),
            Some(FaultAction::Block(state)) => {
                let mut status = state
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                status.0 = true;
                state.changed.notify_all();
                while !status.1 {
                    status = state
                        .changed
                        .wait(status)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                Ok(())
            }
        }
    }
}

#[cfg(not(feature = "test-faults"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FaultController;

#[cfg(not(feature = "test-faults"))]
impl FaultController {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) const fn check(self, _point: FaultPoint) -> std::io::Result<()> {
        Ok(())
    }
}
