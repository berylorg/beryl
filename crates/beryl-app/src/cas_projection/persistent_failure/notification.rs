use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};

#[cfg(all(test, feature = "test-faults"))]
use std::{collections::HashMap, sync::LazyLock};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;

use super::{
    ProjectionServiceGeneration,
    gate::{FailureObservationElection, GateInner, LiveCommandAdmissionError},
};

mod flight;
pub(in crate::cas_projection) use flight::persistent_failure_notification_channel;

/// Exact completion published by the sole running-session recovery supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum RecoverySupervisorFlightCompletion {
    /// Verification kept this exact home and service generation current.
    VerifiedCurrent,
    /// Verification failed or the registered service epoch became stale.
    FailedOrStale,
    /// Shutdown or unavailable supervisor authority ended the flight.
    ShutdownOrUnavailable,
}

#[derive(Clone, Debug)]
pub(super) enum VerificationJoinDisposition {
    Waiting(Arc<VerificationCompletionCell>),
    NotVerification,
    AuthorityLost,
}

/// Closed result of one nonblocking persistent-failure health observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureNotificationStatus {
    /// Exact verifying health was offered to the process recovery supervisor.
    VerificationSignaled,
    /// The exact verification signal joined an already pending or executing recovery flight.
    VerificationJoined,
    /// Exact failed health was offered to the dedicated one-shot worker.
    Signaled,
    /// The exact signal joined an already pending or executing cut.
    Joined,
    /// Typed health did not establish failure of this exact home generation.
    NotFailed,
    /// The retained home or one-shot worker is no longer available.
    Unavailable,
}

/// Cloneable, nonblocking notification handle for exact typed home failure.
#[derive(Clone, Debug)]
pub struct PersistentFailureNotification {
    home: Weak<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    signal: mpsc::SyncSender<()>,
    recovery_flight: Arc<RecoverySupervisorFlight>,
    gate: Arc<GateInner>,
}

#[derive(Debug)]
struct RecoverySupervisorFlight {
    state: Mutex<RecoverySupervisorFlightState>,
}

#[derive(Debug)]
struct RecoverySupervisorFlightState {
    signal: Option<mpsc::SyncSender<()>>,
    active: Option<Arc<VerificationCompletionCell>>,
    next: Option<Arc<VerificationCompletionCell>>,
    last_issued_epoch: u64,
    followup_requested: bool,
    terminal_completion: Option<RecoverySupervisorFlightCompletion>,
}

#[cfg(all(test, feature = "test-faults"))]
#[derive(Debug)]
struct VerificationJoinObservationHook {
    observed: mpsc::SyncSender<()>,
    resume: mpsc::Receiver<()>,
}

#[cfg(all(test, feature = "test-faults"))]
static VERIFICATION_JOIN_OBSERVATION_HOOKS: LazyLock<
    Mutex<HashMap<usize, VerificationJoinObservationHook>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub(super) struct VerificationCompletionCell {
    epoch: u64,
    outcome: Mutex<Option<RecoverySupervisorFlightCompletion>>,
    completed: Condvar,
}

/// Exact immutable completion captured by the supervisor before it wakes scheduler lanes.
#[derive(Debug)]
pub(in crate::cas_projection) struct CompletedRecoverySupervisorFlight {
    cell: Arc<VerificationCompletionCell>,
}

mod lifecycle;
#[cfg(all(test, feature = "test-faults"))]
pub(super) mod test_support;

#[cfg(all(test, feature = "test-faults"))]
mod tests;
