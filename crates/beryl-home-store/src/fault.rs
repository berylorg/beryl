#[cfg(feature = "test-faults")]
use std::{
    any::{TypeId, type_name},
    collections::{HashMap, VecDeque},
    io,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[cfg(feature = "test-faults")]
mod corruption;

#[cfg(feature = "test-faults")]
mod maintenance;

#[cfg(feature = "test-faults")]
pub use corruption::{PersistedCorruptionError, PersistedCorruptionStage};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FaultPoint {
    BeforeReadConfirmation,
    BeforeCommit,
    AfterCommitBeforePersist,
    AfterPersist,
    BeforeVerification,
    BeforeReopen,
    AfterReopen,
    BeforeSidecarRootDirectorySync,
    BeforeSidecarNamespaceDirectorySync,
    BeforeSidecarShardDirectorySync,
    BeforeSidecarFinalDirectorySync,
    BeforeSidecarWrite,
    BeforeSidecarFileSync,
    BeforeSidecarRename,
    AfterSidecarRename,
    BeforeSidecarVerification,
    BeforeThemeVerification,
    BeforeThemeRead,
    BeforeThemeDocumentWrite,
    BeforeThemeDocumentSync,
    BeforeThemeDocumentReplace,
    AfterThemeDocumentReplace,
    BeforeThemeDocumentRemove,
    BeforeThemeInstalledDirectorySync,
    BeforeThemeManifestWrite,
    BeforeThemeManifestSync,
    BeforeThemeManifestReplace,
    AfterThemeManifestReplace,
    BeforeThemeDirectorySync,
}

/// Exact transient typed-command scope for deterministic fault tests.
#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FaultScope {
    type_id: TypeId,
    type_name: &'static str,
}

#[cfg(feature = "test-faults")]
impl FaultScope {
    /// Identifies one concrete typed mutation without relying on a string label.
    #[must_use]
    pub fn of<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
        }
    }
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FaultActionKey {
    point: FaultPoint,
    scope: Option<FaultScope>,
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
    actions: Arc<Mutex<HashMap<FaultActionKey, VecDeque<FaultAction>>>>,
    free_space: Arc<Mutex<FreeSpaceFaultState>>,
}

#[cfg(feature = "test-faults")]
#[derive(Debug, Default)]
struct FreeSpaceFaultState {
    observations: VecDeque<FreeSpaceTestObservation>,
    observation_count: usize,
}

/// One deterministic result from the physical free-space observation boundary.
#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreeSpaceTestObservation {
    /// A platform availability tuple for the canonical home filesystem.
    Observed {
        /// Bytes available to the current caller.
        available_bytes: u64,
        /// Total free bytes reported by the filesystem.
        total_free_bytes: u64,
        /// Total capacity bytes reported by the filesystem.
        total_bytes: u64,
    },
    /// A platform call that supplies no availability observation.
    Unavailable,
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
        self.push(point, None, FaultAction::Error(kind));
    }

    /// Makes the next visit by one exact typed current-domain mutation fail.
    pub fn fail_next_in_scope(&self, point: FaultPoint, scope: FaultScope) {
        self.push(point, Some(scope), FaultAction::Error(io::ErrorKind::Other));
    }

    /// Makes the next `count` visits return synthetic I/O failures.
    pub fn fail_times(&self, point: FaultPoint, count: usize) {
        self.fail_times_with_kind(point, io::ErrorKind::Other, count);
    }

    /// Makes the next `count` visits return failures with the exact I/O kind.
    pub fn fail_times_with_kind(&self, point: FaultPoint, kind: io::ErrorKind, count: usize) {
        for _ in 0..count {
            self.push(point, None, FaultAction::Error(kind));
        }
    }

    /// Makes the next visit panic on the thread executing the store operation.
    pub fn panic_next(&self, point: FaultPoint) {
        self.push(point, None, FaultAction::Panic);
    }

    /// Makes the next visit terminate its subprocess immediately.
    pub fn abort_next(&self, point: FaultPoint) {
        self.push(point, None, FaultAction::Abort);
    }

    /// Blocks the next visit until the returned handle releases it.
    #[must_use]
    pub fn block_next(&self, point: FaultPoint) -> FaultBlock {
        let state = Arc::new(BlockState::default());
        self.push(point, None, FaultAction::Block(state.clone()));
        FaultBlock { state }
    }

    /// Blocks the next visit by one exact typed current-domain mutation.
    #[must_use]
    pub fn block_next_in_scope(&self, point: FaultPoint, scope: FaultScope) -> FaultBlock {
        let state = Arc::new(BlockState::default());
        self.push(point, Some(scope), FaultAction::Block(state.clone()));
        FaultBlock { state }
    }

    /// Supplies one deterministic physical free-space observation.
    ///
    /// The result is consumed in FIFO order by [`crate::HomeStore::query_free_space`].
    /// It replaces only the platform observation at that boundary and does not
    /// introduce production query state, caching, or retries.
    pub fn push_free_space_observation(&self, observation: FreeSpaceTestObservation) {
        self.free_space
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observations
            .push_back(observation);
    }

    /// Returns the exact number of free-space physical-boundary observations.
    #[must_use]
    pub fn free_space_observation_count(&self) -> usize {
        self.free_space
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observation_count
    }

    fn push(&self, point: FaultPoint, scope: Option<FaultScope>, action: FaultAction) {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(FaultActionKey { point, scope })
            .or_default()
            .push_back(action);
    }

    pub(crate) fn free_space_observation(&self) -> Option<FreeSpaceTestObservation> {
        let mut state = self
            .free_space
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observation_count = state
            .observation_count
            .checked_add(1)
            .expect("free-space observation counter exhausted");
        state.observations.pop_front()
    }

    pub(crate) fn check(&self, point: FaultPoint) -> io::Result<()> {
        self.check_action(point, None)
    }

    pub(crate) fn check_current(&self, point: FaultPoint, scope: FaultScope) -> io::Result<()> {
        let action = {
            let mut actions = self
                .actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            actions
                .get_mut(&FaultActionKey {
                    point,
                    scope: Some(scope),
                })
                .and_then(VecDeque::pop_front)
                .or_else(|| {
                    actions
                        .get_mut(&FaultActionKey { point, scope: None })
                        .and_then(VecDeque::pop_front)
                })
        };
        Self::apply(point, Some(scope), action)
    }

    fn check_action(&self, point: FaultPoint, scope: Option<FaultScope>) -> io::Result<()> {
        let action = self
            .actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&FaultActionKey { point, scope })
            .and_then(VecDeque::pop_front);
        Self::apply(point, scope, action)
    }

    fn apply(
        point: FaultPoint,
        scope: Option<FaultScope>,
        action: Option<FaultAction>,
    ) -> io::Result<()> {
        let location = scope.map_or_else(
            || format!("{point:?}"),
            |scope| format!("{point:?} for {}", scope.type_name),
        );
        match action {
            None => Ok(()),
            Some(FaultAction::Error(kind)) => Err(io::Error::new(
                kind,
                format!("synthetic Beryl-home fault at {location}"),
            )),
            Some(FaultAction::Panic) => {
                panic!("synthetic Beryl-home panic at {location}")
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
