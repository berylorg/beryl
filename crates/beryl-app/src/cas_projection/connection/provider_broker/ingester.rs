use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[cfg(feature = "test-faults")]
use std::sync::OnceLock;

use beryl_backend::{
    ClientUserMessageId, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
};
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::{BerylHomeId, ProviderObservationId};
use beryl_stream::{
    ChannelBuildError, PagePool, PagePoolDiagnostics, PagePoolError, fixed_channel,
};
use syndic_storage::{SyndicDeliveringSteeringInput, SyndicStorage};
use thiserror::Error;

#[cfg(test)]
use super::CheckedSteeringLifecycle;
use super::{
    CheckedSteeringLifecycleArmError, CheckedSteeringLifecycleOwner, PROVIDER_PAGE_BYTES,
    approval::{ApprovalInterruptionSlot, PendingApprovalInterruption},
    channel::{AckSlot, BrokerOperation, BrokerReceiver, BrokerReply, BrokerSink},
    steering_result::SteeringResultSlot,
};
use crate::cas_projection::{
    LiveCommandAuthorizer, LiveCommandPermit, PersistentFailureNotification,
    connection::{
        ConnectionRegistryAuthority,
        driver_outcome::ConnectionRoutingFailure,
        registry,
        router::{
            ActiveSteeringAttemptPermit, EventRouter, LiveEventTargetCloseReason,
            TargetInvalidation,
        },
    },
    initial_start::InitialStartGate,
    service_config::ProjectionWorkerPermit,
};

mod activation;
mod active;
mod approval;
mod compaction;
mod construction;
mod control;
mod core;
mod disposition;
mod dynamic;
mod lifecycle;
mod operations;
mod state;
mod steering_user;
mod submitted_user;
mod terminal;

use active::{ActiveDynamicTool, ActiveIngress, ActiveObservation, ActiveSteeringLifecycle};
use state::{
    StickyRoutingFailure, TargetRouteOutcome, WholeConnectionRoutingFailure, receive_next,
};

#[derive(Debug)]
enum CurrentObservationAuthorityError {
    HomeIdentity {
        expected: BerylHomeId,
        actual: BerylHomeId,
    },
    Health(beryl_home_store::HomeHealthSnapshot),
    Generation {
        expected: HomeGeneration,
        actual: Option<HomeGeneration>,
    },
    Reacquire(beryl_home_store::DomainHandleError),
    Changed {
        before: beryl_home_store::HomeHealthSnapshot,
        after: beryl_home_store::HomeHealthSnapshot,
    },
}

impl CurrentObservationAuthorityError {
    fn verification_ambiguous(&self, expected_generation: HomeGeneration) -> bool {
        let snapshot_matches = |snapshot: &beryl_home_store::HomeHealthSnapshot| {
            snapshot.state() == HomeHealthState::Verifying
                && snapshot.generation() == Some(expected_generation)
        };
        match self {
            Self::Health(snapshot) => snapshot_matches(snapshot),
            Self::Changed { after, .. } => snapshot_matches(after),
            Self::Reacquire(beryl_home_store::DomainHandleError::HealthGate(source)) => {
                source.state() == HomeHealthState::Verifying
                    && source.generation() == expected_generation
            }
            _ => false,
        }
    }
}

#[derive(Debug, Error)]
enum ProviderBrokerBuildFailure {
    #[error(transparent)]
    Channel(#[from] ChannelBuildError),
    #[error(transparent)]
    PagePool(#[from] PagePoolError),
    #[error("failed to start the provider observation ingester: {message}")]
    Spawn { message: String },
}

pub(in crate::cas_projection::connection) struct ProviderBrokerBuildError {
    failure: ProviderBrokerBuildFailure,
    resources: ProviderBrokerBuildResources,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderBrokerStartState {
    Blocked,
    Started,
    Cancelled,
}

struct ProviderBrokerStartGate {
    state: Mutex<ProviderBrokerStartState>,
    changed: Condvar,
}

struct ProviderBrokerWorkerOwner {
    state: Mutex<ProviderBrokerWorkerState>,
}

struct ProviderBrokerWorkerState {
    terminal: bool,
    disposition: ProviderBrokerWorkerDisposition,
    worker: Option<ProjectionWorkerPermit>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderBrokerWorkerDisposition {
    Undecided,
    OrdinaryRelease,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(in crate::cas_projection::connection) enum ProviderBrokerWorkerDispositionArmError {
    #[error("the provider-ingester worker disposition state is poisoned")]
    Poisoned,
}

struct ProviderBrokerWorkerTerminalGuard {
    worker: Arc<ProviderBrokerWorkerOwner>,
}

struct ProviderBrokerJoinedWorker {
    worker: Option<ProjectionWorkerPermit>,
    disposition: Option<ProviderBrokerWorkerDisposition>,
}

enum ProviderBrokerBuildResources {
    Worker {
        worker: Arc<ProviderBrokerWorkerOwner>,
    },
    PagePool {
        worker: Arc<ProviderBrokerWorkerOwner>,
        pages: PagePool,
    },
    Unstarted(ProviderBrokerUnstarted),
}

struct ProviderBrokerUnstarted {
    sink: BrokerSink,
    control: Arc<ProviderBrokerControl>,
    launch: Arc<ProviderBrokerLaunchEscrow>,
    start_gate: Arc<ProviderBrokerStartGate>,
    initial_start: Arc<InitialStartGate>,
    worker: Arc<ProviderBrokerWorkerOwner>,
}

struct ProviderBrokerLaunchEscrow {
    ingester: Mutex<Option<Ingester>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderBrokerBuildFault {
    None,
    #[cfg(test)]
    PagePool,
    #[cfg(test)]
    Channel,
    #[cfg(test)]
    Spawn,
}

#[cfg(any(test, feature = "test-faults"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProviderBrokerBuildResourceSnapshot {
    worker: bool,
    pages: bool,
    channel: bool,
    sink: bool,
    control: bool,
    ingester: bool,
    start_gate: bool,
    initial_start: bool,
}

pub(in crate::cas_projection::connection) struct ProviderBrokerStartToken {
    gate: Arc<ProviderBrokerStartGate>,
    control: Arc<ProviderBrokerControl>,
    active: bool,
}

pub(in crate::cas_projection::connection) struct StartBlockedProviderBrokerIngester {
    control: Arc<ProviderBrokerControl>,
    worker: Arc<ProviderBrokerWorkerOwner>,
    handle: Option<JoinHandle<ProviderBrokerTerminalReceipt>>,
    initial_start: Arc<InitialStartGate>,
}

pub(in crate::cas_projection::connection) struct RunningProviderBrokerIngester {
    control: Arc<ProviderBrokerControl>,
    worker: Arc<ProviderBrokerWorkerOwner>,
    handle: Option<JoinHandle<ProviderBrokerTerminalReceipt>>,
    active: bool,
}

pub(in crate::cas_projection::connection) struct PreparedProviderBroker {
    pub(in crate::cas_projection::connection) sink: Box<dyn OrderedTurnStreamSink>,
    pub(in crate::cas_projection::connection) control: Arc<ProviderBrokerControl>,
    pub(in crate::cas_projection::connection) ingester: StartBlockedProviderBrokerIngester,
    pub(in crate::cas_projection::connection) start: ProviderBrokerStartToken,
}

pub(in crate::cas_projection::connection) struct ProviderBroker;

pub(in crate::cas_projection::connection) struct ProviderBrokerControl {
    pub(super) home: Arc<HomeStore>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) authority: Arc<ConnectionRegistryAuthority>,
    pub(super) router: Arc<EventRouter>,
    pub(super) stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
    pub(super) context_compaction:
        Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator>,
    pub(super) commands: LiveCommandAuthorizer,
    pub(super) failure_notification: PersistentFailureNotification,
    cancelled: Arc<AtomicBool>,
    ack: Arc<AckSlot>,
    routing_failure: Arc<StickyRoutingFailure>,
    approval: Arc<ApprovalInterruptionSlot>,
    steering_results: Arc<SteeringResultSlot>,
    pages: PagePool,
    pub(super) loss: Mutex<()>,
    #[cfg(feature = "test-faults")]
    test_metrics: Arc<crate::cas_projection::test_faults::ProviderBrokerTestMetrics>,
}

pub(in crate::cas_projection) struct ProviderBrokerStopped {
    worker: Option<ProjectionWorkerPermit>,
    worker_disposition: Option<ProviderBrokerWorkerDisposition>,
    receipt: ProviderBrokerTerminalReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::connection) struct ProviderBrokerTerminalReceipt {
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
    home_generation: HomeGeneration,
    clean: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(in crate::cas_projection::connection) enum ProviderBrokerIngesterJoinError {
    #[error("the provider ingester failed or panicked before clean termination")]
    WorkerFailed,
    #[error("the provider ingester terminal receipt belongs to another service attachment")]
    EpochMismatch,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ForcedProviderBrokerJoinFailure {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
}

#[cfg(feature = "test-faults")]
static FORCED_PROVIDER_BROKER_JOIN_FAILURES: OnceLock<Mutex<Vec<ForcedProviderBrokerJoinFailure>>> =
    OnceLock::new();

#[derive(Clone, Debug)]
pub(in crate::cas_projection::connection) enum ProviderBrokerResponseActivationFailure {
    Target(LiveEventTargetCloseReason),
    Router,
}

struct Ingester {
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
    context_compaction:
        Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator>,
    commands: LiveCommandAuthorizer,
    failure_notification: PersistentFailureNotification,
    receiver: BrokerReceiver,
    ack: Arc<AckSlot>,
    cancelled: Arc<AtomicBool>,
    routing_failure: Arc<StickyRoutingFailure>,
    approval: Arc<ApprovalInterruptionSlot>,
    steering_results: Arc<SteeringResultSlot>,
    pages: PagePool,
    command: Option<LiveCommandPermit>,
    active: Option<ActiveIngress>,
    authority_lost: bool,
    #[cfg(feature = "test-faults")]
    test_metrics: Arc<crate::cas_projection::test_faults::ProviderBrokerTestMetrics>,
}

#[cfg(feature = "test-faults")]
pub(in crate::cas_projection::connection) fn fail_next_provider_broker_join_for_test(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
) {
    let failure = ForcedProviderBrokerJoinFailure {
        home_id,
        home_generation,
        service_generation,
    };
    let mut failures = FORCED_PROVIDER_BROKER_JOIN_FAILURES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("the provider-ingester join-failure test hook is usable");
    assert!(
        !failures.contains(&failure),
        "an exact provider-ingester join failure is armed once"
    );
    failures.push(failure);
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_ingester.rs"
    ));
}
