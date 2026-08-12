use std::sync::{Arc, Condvar, Mutex};

#[cfg(feature = "test-faults")]
use std::time::{Duration, Instant};

use crate::DomainValidationError;

pub(crate) type SharedScrubResult = Result<(), Arc<DomainValidationError>>;

/// Why a caller requested exhaustive whole-home validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WholeHomeScrubTrigger {
    /// An explicit user or diagnostic request.
    Explicit,
    /// Bounded background maintenance.
    Background,
    /// Independently observed corruption evidence.
    CorruptionEvidence,
}

#[derive(Default)]
struct FlightState {
    pending_rerun: bool,
    rerun_started: bool,
    result: Option<SharedScrubResult>,
}

struct ScrubFlight {
    state: Mutex<FlightState>,
    completed: Condvar,
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
struct TerminalDecisionState {
    armed: bool,
    reached: bool,
    released: bool,
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
struct TerminalDecisionSeam {
    state: Mutex<TerminalDecisionState>,
    changed: Condvar,
}

#[cfg(feature = "test-faults")]
impl TerminalDecisionSeam {
    fn arm(self: &Arc<Self>) -> ScrubTerminalDecisionBlock {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = TerminalDecisionState {
            armed: true,
            reached: false,
            released: false,
        };
        ScrubTerminalDecisionBlock {
            seam: Arc::clone(self),
        }
    }

    fn block_if_armed(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.armed {
            return;
        }
        state.armed = false;
        state.reached = true;
        self.changed.notify_all();
        while !state.released {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Deterministic test-only block at scrub terminal decision while `active` is held.
#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct ScrubTerminalDecisionBlock {
    seam: Arc<TerminalDecisionSeam>,
}

#[cfg(feature = "test-faults")]
impl ScrubTerminalDecisionBlock {
    /// Waits until the first worker has returned and terminal retirement owns `active`.
    #[must_use]
    pub fn wait_until_reached(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .seam
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !state.reached {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, wait) = self
                .seam
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            if wait.timed_out() && !state.reached {
                return false;
            }
        }
        true
    }

    /// Releases terminal retirement.
    pub fn release(&self) {
        let mut state = self
            .seam
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.released = true;
        self.seam.changed.notify_all();
    }
}

impl ScrubFlight {
    fn new() -> Self {
        Self {
            state: Mutex::new(FlightState::default()),
            completed: Condvar::new(),
        }
    }

    fn note_corruption_evidence(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.rerun_started {
            let changed = !state.pending_rerun;
            state.pending_rerun = true;
            changed
        } else {
            false
        }
    }

    fn take_coalesced_rerun(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.pending_rerun && !state.rerun_started {
            state.pending_rerun = false;
            state.rerun_started = true;
            true
        } else {
            false
        }
    }

    fn publish(&self, result: SharedScrubResult) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending_rerun = false;
        state.result = Some(result);
        self.completed.notify_all();
    }

    fn join(&self) -> SharedScrubResult {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while state.result.is_none() {
            state = self
                .completed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.result.clone().expect("scrub flight has a result")
    }
}

#[derive(Default)]
pub(crate) struct ScrubCoordinator {
    active: Mutex<Option<Arc<ScrubFlight>>>,
    #[cfg(feature = "test-faults")]
    terminal_decision: Arc<TerminalDecisionSeam>,
    #[cfg(feature = "test-faults")]
    requests_entered: std::sync::atomic::AtomicUsize,
    #[cfg(feature = "test-faults")]
    joined: std::sync::atomic::AtomicUsize,
    #[cfg(feature = "test-faults")]
    coalesced_reruns: std::sync::atomic::AtomicUsize,
    #[cfg(feature = "test-faults")]
    worker_runs: std::sync::atomic::AtomicUsize,
}

impl ScrubCoordinator {
    pub(crate) fn run(
        &self,
        trigger: WholeHomeScrubTrigger,
        operation: impl Fn() -> Result<(), DomainValidationError> + Sync,
    ) -> SharedScrubResult {
        #[cfg(feature = "test-faults")]
        self.requests_entered
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (flight, leader) = {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match active.as_ref() {
                Some(flight) => {
                    #[cfg(feature = "test-faults")]
                    self.joined
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let flight = Arc::clone(flight);
                    if trigger == WholeHomeScrubTrigger::CorruptionEvidence {
                        let coalesced = flight.note_corruption_evidence();
                        #[cfg(feature = "test-faults")]
                        if coalesced {
                            self.coalesced_reruns
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    (flight, false)
                }
                None => {
                    let flight = Arc::new(ScrubFlight::new());
                    *active = Some(Arc::clone(&flight));
                    (flight, true)
                }
            }
        };

        if !leader {
            return flight.join();
        }

        let execute = || {
            #[cfg(feature = "test-faults")]
            self.worker_runs
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::scope(|scope| scope.spawn(&operation).join())
                .unwrap_or(Err(DomainValidationError::WorkerPanicked))
                .map_err(Arc::new)
        };
        let mut result = execute();

        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let rerun = flight.take_coalesced_rerun();
        if !rerun {
            #[cfg(feature = "test-faults")]
            self.terminal_decision.block_if_armed();
            retire_and_publish(&mut active, &flight, result.clone());
            return result;
        }
        drop(active);

        // Evidence arriving during this one coalesced rerun joins the same
        // flight but cannot request another rerun because `rerun_started` is set.
        result = execute();

        // The rerun worker has joined, so all worker-owned resources are gone.
        // Retire and publish atomically before another request may observe idle.
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        retire_and_publish(&mut active, &flight, result.clone());
        result
    }
}

fn retire_and_publish(
    active: &mut Option<Arc<ScrubFlight>>,
    flight: &Arc<ScrubFlight>,
    result: SharedScrubResult,
) {
    if active
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, flight))
    {
        *active = None;
    }
    flight.publish(result);
}

/// Bounded test-only observation of one home's scrub coordinator.
#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrubTestSnapshot {
    /// Whether one flight currently owns the per-home worker marker.
    pub active: bool,
    /// Number of requests that joined an already active flight.
    pub joined: usize,
    /// Number of pending-rerun flags installed by corruption evidence.
    pub coalesced_reruns: usize,
    /// Number of isolated worker runs completed or active.
    pub worker_runs: usize,
}

#[cfg(feature = "test-faults")]
impl crate::HomeStore {
    /// Blocks the next no-rerun terminal decision while the coordinator owns `active`.
    #[must_use]
    pub fn block_next_scrub_terminal_decision(&self) -> ScrubTerminalDecisionBlock {
        self.scrub.terminal_decision.arm()
    }

    /// Counts requests that entered the coordinator before attempting `active`.
    #[must_use]
    pub fn scrub_test_requests_entered(&self) -> usize {
        self.scrub
            .requests_entered
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Observes scrub single-flight counters without exposing production control.
    #[must_use]
    pub fn scrub_test_snapshot(&self) -> ScrubTestSnapshot {
        use std::sync::atomic::Ordering;

        ScrubTestSnapshot {
            active: self
                .scrub
                .active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some(),
            joined: self.scrub.joined.load(Ordering::SeqCst),
            coalesced_reruns: self.scrub.coalesced_reruns.load(Ordering::SeqCst),
            worker_runs: self.scrub.worker_runs.load(Ordering::SeqCst),
        }
    }
}
