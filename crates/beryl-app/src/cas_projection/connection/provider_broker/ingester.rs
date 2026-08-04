use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[cfg(test)]
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
    persistent_failure::PersistentFailureCutIdentity,
    service_config::ProjectionWorkerPermit,
    service_startup::ServiceStartupGate,
};

mod activation;
mod active;
mod approval;
mod compaction;
mod dynamic;
mod operations;
mod state;
mod steering_user;
mod submitted_user;
mod terminal;

use active::{
    ActiveDurableObservation, ActiveDynamicTool, ActiveIngress, ActiveObservation,
    ActiveSteeringLifecycle,
};
use state::{
    StickyRoutingFailure, TargetRouteOutcome, WholeConnectionRoutingFailure, receive_next,
};

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

impl ProviderBrokerBuildError {
    fn new(failure: ProviderBrokerBuildFailure, resources: ProviderBrokerBuildResources) -> Self {
        Self { failure, resources }
    }

    #[cfg(test)]
    fn resource_snapshot(&self) -> ProviderBrokerBuildResourceSnapshot {
        self.resources.snapshot()
    }
}

#[cfg(test)]
impl ProviderBrokerBuildResources {
    fn snapshot(&self) -> ProviderBrokerBuildResourceSnapshot {
        match self {
            Self::Worker { worker } => ProviderBrokerBuildResourceSnapshot {
                worker: worker.retains_worker(),
                pages: false,
                channel: false,
                sink: false,
                control: false,
                ingester: false,
                start_gate: false,
                startup_gate: false,
            },
            Self::PagePool { worker, pages } => {
                let diagnostics = pages.diagnostics();
                ProviderBrokerBuildResourceSnapshot {
                    worker: worker.retains_worker(),
                    pages: diagnostics.page_count == 1,
                    channel: false,
                    sink: false,
                    control: false,
                    ingester: false,
                    start_gate: false,
                    startup_gate: false,
                }
            }
            Self::Unstarted(unstarted) => ProviderBrokerBuildResourceSnapshot {
                worker: unstarted.worker.retains_worker(),
                pages: unstarted.control.pages.diagnostics().page_count == 1,
                channel: unstarted.launch.retains_ingester(),
                sink: true,
                control: true,
                ingester: unstarted.launch.retains_ingester(),
                start_gate: true,
                startup_gate: true,
            },
        }
    }
}

impl std::fmt::Display for ProviderBrokerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.failure.fmt(formatter)
    }
}

impl std::fmt::Debug for ProviderBrokerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderBrokerBuildError")
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl std::error::Error for ProviderBrokerBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failure.source()
    }
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

struct ProviderBrokerWorkerEscrow {
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
    RetainForAdoption(PersistentFailureCutIdentity),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(in crate::cas_projection::connection) enum ProviderBrokerWorkerDispositionArmError {
    #[error("the provider-ingester worker disposition state is poisoned")]
    Poisoned,
    #[error("the provider-ingester worker already has an incompatible terminal disposition")]
    Contended,
}

struct ProviderBrokerWorkerTerminalGuard {
    worker: Arc<ProviderBrokerWorkerEscrow>,
}

struct ProviderBrokerJoinedWorker {
    worker: Option<ProjectionWorkerPermit>,
    disposition: Option<ProviderBrokerWorkerDisposition>,
}

enum ProviderBrokerBuildResources {
    Worker {
        worker: Arc<ProviderBrokerWorkerEscrow>,
    },
    PagePool {
        worker: Arc<ProviderBrokerWorkerEscrow>,
        pages: PagePool,
    },
    Unstarted(ProviderBrokerUnstarted),
}

struct ProviderBrokerUnstarted {
    sink: BrokerSink,
    control: Arc<ProviderBrokerControl>,
    launch: Arc<ProviderBrokerLaunchEscrow>,
    start_gate: Arc<ProviderBrokerStartGate>,
    startup_gate: Arc<ServiceStartupGate>,
    worker: Arc<ProviderBrokerWorkerEscrow>,
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

impl ProviderBrokerBuildFault {
    fn page_pool_failure(self) -> Option<PagePoolError> {
        #[cfg(test)]
        if self == Self::PagePool {
            return Some(PagePoolError::AllocationFailed);
        }
        None
    }

    fn channel_failure(self) -> Option<ChannelBuildError> {
        #[cfg(test)]
        if self == Self::Channel {
            return Some(ChannelBuildError::AllocationFailed);
        }
        None
    }

    fn spawn_failure(self) -> Option<std::io::Error> {
        #[cfg(test)]
        if self == Self::Spawn {
            return Some(std::io::Error::other(
                "injected provider broker spawn failure",
            ));
        }
        None
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProviderBrokerBuildResourceSnapshot {
    worker: bool,
    pages: bool,
    channel: bool,
    sink: bool,
    control: bool,
    ingester: bool,
    start_gate: bool,
    startup_gate: bool,
}

pub(in crate::cas_projection::connection) struct ProviderBrokerStartToken {
    gate: Arc<ProviderBrokerStartGate>,
    control: Arc<ProviderBrokerControl>,
    active: bool,
}

pub(in crate::cas_projection::connection) struct StartBlockedProviderBrokerIngester {
    control: Arc<ProviderBrokerControl>,
    worker: Arc<ProviderBrokerWorkerEscrow>,
    handle: Option<JoinHandle<ProviderBrokerTerminalReceipt>>,
    startup_gate: Arc<ServiceStartupGate>,
}

pub(in crate::cas_projection::connection) struct RunningProviderBrokerIngester {
    control: Arc<ProviderBrokerControl>,
    worker: Arc<ProviderBrokerWorkerEscrow>,
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

impl ProviderBroker {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn prepare(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        Self::prepare_with_startup_gate(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
            ServiceStartupGate::open_gate(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn prepare_with_startup_gate(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
        startup_gate: Arc<ServiceStartupGate>,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        Self::prepare_with_startup_gate_inner(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
            startup_gate,
            ProviderBrokerBuildFault::None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_with_startup_gate_inner(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
        startup_gate: Arc<ServiceStartupGate>,
        build_fault: ProviderBrokerBuildFault,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        let worker = Arc::new(ProviderBrokerWorkerEscrow::new(worker));
        if let Some(error) = build_fault.page_pool_failure() {
            return Err(ProviderBrokerBuildError::new(
                ProviderBrokerBuildFailure::PagePool(error),
                ProviderBrokerBuildResources::Worker { worker },
            ));
        }
        let pages = match PagePool::new(
            NonZeroUsize::new(PROVIDER_PAGE_BYTES).expect("provider page size is nonzero"),
            NonZeroUsize::MIN,
        ) {
            Ok(pages) => pages,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::PagePool(error),
                    ProviderBrokerBuildResources::Worker { worker },
                ));
            }
        };
        if let Some(error) = build_fault.channel_failure() {
            return Err(ProviderBrokerBuildError::new(
                ProviderBrokerBuildFailure::Channel(error),
                ProviderBrokerBuildResources::PagePool { worker, pages },
            ));
        }
        let (sender, receiver) = match fixed_channel(NonZeroUsize::MIN) {
            Ok(channel) => channel,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::Channel(error),
                    ProviderBrokerBuildResources::PagePool { worker, pages },
                ));
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let ack = Arc::new(AckSlot::new());
        let routing_failure = Arc::new(StickyRoutingFailure::default());
        let approval = Arc::new(ApprovalInterruptionSlot::new());
        let steering_results = Arc::new(SteeringResultSlot::default());
        #[cfg(feature = "test-faults")]
        let test_metrics =
            Arc::new(crate::cas_projection::test_faults::ProviderBrokerTestMetrics::default());
        let sink = BrokerSink::new(
            sender,
            Arc::clone(&ack),
            Arc::clone(&cancelled),
            #[cfg(feature = "test-faults")]
            home_id,
            #[cfg(feature = "test-faults")]
            Arc::clone(&test_metrics),
        );
        let control = Arc::new(ProviderBrokerControl {
            home: Arc::clone(&home),
            home_id,
            home_generation,
            authority: Arc::clone(&authority),
            router: Arc::clone(&router),
            stop_coordinator: Arc::clone(&stop_coordinator),
            context_compaction: Arc::clone(&context_compaction),
            commands: commands.clone(),
            failure_notification: failure_notification.clone(),
            cancelled: Arc::clone(&cancelled),
            ack: Arc::clone(&ack),
            routing_failure: Arc::clone(&routing_failure),
            approval: Arc::clone(&approval),
            steering_results: Arc::clone(&steering_results),
            pages: pages.clone(),
            loss: Mutex::new(()),
            #[cfg(feature = "test-faults")]
            test_metrics: Arc::clone(&test_metrics),
        });
        let launch = Arc::new(ProviderBrokerLaunchEscrow {
            ingester: Mutex::new(Some(Ingester {
                home: Arc::clone(&home),
                home_id,
                home_generation,
                authority: Arc::clone(&authority),
                router: Arc::clone(&router),
                stop_coordinator: Arc::clone(&stop_coordinator),
                context_compaction: Arc::clone(&context_compaction),
                commands: commands.clone(),
                failure_notification: failure_notification.clone(),
                receiver,
                ack: Arc::clone(&ack),
                cancelled: Arc::clone(&cancelled),
                routing_failure: Arc::clone(&routing_failure),
                approval: Arc::clone(&approval),
                steering_results: Arc::clone(&steering_results),
                pages,
                command: None,
                active: None,
                authority_lost: false,
                #[cfg(feature = "test-faults")]
                test_metrics: Arc::clone(&test_metrics),
            })),
        });
        let start_gate = Arc::new(ProviderBrokerStartGate::new());
        ProviderBrokerUnstarted {
            sink,
            control,
            launch,
            start_gate,
            startup_gate,
            worker,
        }
        .spawn(authority.generation.get(), build_fault)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
    ) -> Result<
        (
            Box<dyn OrderedTurnStreamSink>,
            Arc<ProviderBrokerControl>,
            RunningProviderBrokerIngester,
        ),
        ProviderBrokerBuildError,
    > {
        let prepared = Self::prepare(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
        )?;
        let PreparedProviderBroker {
            sink,
            control,
            ingester,
            start,
        } = prepared;
        Ok((sink, control, ingester.start(start)))
    }
}

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

pub(in crate::cas_projection) struct ProviderBrokerAdoptionStopped {
    worker: ProjectionWorkerPermit,
    receipt: ProviderBrokerTerminalReceipt,
    cut: PersistentFailureCutIdentity,
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
    #[error("the provider ingester terminal receipt belongs to another service epoch")]
    EpochMismatch,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ForcedProviderBrokerJoinFailure {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
}

#[cfg(test)]
static FORCED_PROVIDER_BROKER_JOIN_FAILURES: OnceLock<Mutex<Vec<ForcedProviderBrokerJoinFailure>>> =
    OnceLock::new();

#[derive(Clone, Debug)]
pub(in crate::cas_projection::connection) enum ProviderBrokerResponseActivationFailure {
    Target(LiveEventTargetCloseReason),
    Router,
}

impl ProviderBrokerUnstarted {
    fn spawn(
        self,
        connection_generation: u64,
        build_fault: ProviderBrokerBuildFault,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        let thread_gate = Arc::clone(&self.start_gate);
        let thread_worker = Arc::clone(&self.worker);
        let thread_startup_gate = Arc::clone(&self.startup_gate);
        let thread_launch = Arc::clone(&self.launch);
        let handle = if let Some(error) = build_fault.spawn_failure() {
            Err(error)
        } else {
            std::thread::Builder::new()
                .name(format!("beryl-provider-ingester-{connection_generation}"))
                .spawn(move || {
                    let _worker_terminal = ProviderBrokerWorkerTerminalGuard::new(thread_worker);
                    let ingester = thread_launch.take();
                    if thread_gate.wait_until_started() {
                        if thread_startup_gate.wait() {
                            ingester.run()
                        } else {
                            ingester.cancel_before_start()
                        }
                    } else {
                        ingester.cancel_before_start()
                    }
                })
        };
        let handle = match handle {
            Ok(handle) => handle,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::Spawn {
                        message: error.to_string(),
                    },
                    ProviderBrokerBuildResources::Unstarted(self),
                ));
            }
        };
        let Self {
            sink,
            control,
            launch: _,
            start_gate,
            startup_gate,
            worker,
        } = self;
        Ok(PreparedProviderBroker {
            sink: Box::new(sink),
            control: Arc::clone(&control),
            ingester: StartBlockedProviderBrokerIngester {
                control: Arc::clone(&control),
                worker,
                handle: Some(handle),
                startup_gate,
            },
            start: ProviderBrokerStartToken {
                gate: start_gate,
                control,
                active: true,
            },
        })
    }
}

impl ProviderBrokerLaunchEscrow {
    fn take(&self) -> Ingester {
        self.ingester
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .expect("the provider broker launch escrow owns one ingester")
    }

    #[cfg(test)]
    fn retains_ingester(&self) -> bool {
        self.ingester
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }
}

impl ProviderBrokerStartGate {
    fn new() -> Self {
        Self {
            state: Mutex::new(ProviderBrokerStartState::Blocked),
            changed: Condvar::new(),
        }
    }

    fn wait_until_started(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while *state == ProviderBrokerStartState::Blocked {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        *state == ProviderBrokerStartState::Started
    }

    fn start(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if *state != ProviderBrokerStartState::Blocked {
            return false;
        }
        *state = ProviderBrokerStartState::Started;
        drop(state);
        self.changed.notify_all();
        true
    }

    fn cancel(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if *state == ProviderBrokerStartState::Blocked {
            *state = ProviderBrokerStartState::Cancelled;
        }
        drop(state);
        self.changed.notify_all();
    }
}

impl ProviderBrokerWorkerEscrow {
    fn new(worker: ProjectionWorkerPermit) -> Self {
        Self {
            state: Mutex::new(ProviderBrokerWorkerState {
                terminal: false,
                disposition: ProviderBrokerWorkerDisposition::Undecided,
                worker: Some(worker),
            }),
        }
    }

    fn arm_ordinary_release(&self) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        let release = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ProviderBrokerWorkerDispositionArmError::Poisoned)?;
            match state.disposition {
                ProviderBrokerWorkerDisposition::Undecided => {
                    state.disposition = ProviderBrokerWorkerDisposition::OrdinaryRelease;
                }
                ProviderBrokerWorkerDisposition::OrdinaryRelease => {}
                ProviderBrokerWorkerDisposition::RetainForAdoption(_) => {
                    return Err(ProviderBrokerWorkerDispositionArmError::Contended);
                }
            }
            state.terminal.then(|| state.worker.take()).flatten()
        };
        drop(release);
        Ok(())
    }

    fn arm_retain_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ProviderBrokerWorkerDispositionArmError::Poisoned)?;
        match state.disposition {
            ProviderBrokerWorkerDisposition::Undecided => {
                state.disposition = ProviderBrokerWorkerDisposition::RetainForAdoption(cut);
                Ok(())
            }
            ProviderBrokerWorkerDisposition::RetainForAdoption(existing) if existing == cut => {
                Ok(())
            }
            ProviderBrokerWorkerDisposition::OrdinaryRelease
            | ProviderBrokerWorkerDisposition::RetainForAdoption(_) => {
                Err(ProviderBrokerWorkerDispositionArmError::Contended)
            }
        }
    }

    fn mark_terminal(&self) {
        let release = match self.state.lock() {
            Ok(mut state) => {
                state.terminal = true;
                matches!(
                    state.disposition,
                    ProviderBrokerWorkerDisposition::OrdinaryRelease
                )
                .then(|| state.worker.take())
                .flatten()
            }
            Err(poison) => {
                // Poison cannot prove which disposition won. Record terminal progress while
                // retaining the exact admission for a later consuming owner.
                poison.into_inner().terminal = true;
                None
            }
        };
        drop(release);
    }

    fn take_joined_worker(&self) -> ProviderBrokerJoinedWorker {
        match self.state.lock() {
            Ok(mut state) => ProviderBrokerJoinedWorker {
                worker: state.worker.take(),
                disposition: Some(state.disposition),
            },
            Err(poison) => ProviderBrokerJoinedWorker {
                worker: poison.into_inner().worker.take(),
                disposition: None,
            },
        }
    }

    #[cfg(test)]
    fn retains_worker(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .worker
            .is_some()
    }
}

impl ProviderBrokerWorkerTerminalGuard {
    fn new(worker: Arc<ProviderBrokerWorkerEscrow>) -> Self {
        Self { worker }
    }
}

impl Drop for ProviderBrokerWorkerTerminalGuard {
    fn drop(&mut self) {
        self.worker.mark_terminal();
    }
}

impl ProviderBrokerStartToken {
    fn activate(mut self) {
        assert!(
            self.gate.start(),
            "provider ingester start token is one-shot"
        );
        self.active = false;
    }

    fn try_activate(mut self) -> Result<(), Self> {
        if !self.gate.start() {
            return Err(self);
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for ProviderBrokerStartToken {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.control.request_cancel();
        self.gate.cancel();
    }
}

impl StartBlockedProviderBrokerIngester {
    fn start(mut self, token: ProviderBrokerStartToken) -> RunningProviderBrokerIngester {
        assert!(
            Arc::ptr_eq(&self.control, &token.control),
            "provider ingester start token belongs to its exact broker"
        );
        token.activate();
        RunningProviderBrokerIngester {
            control: Arc::clone(&self.control),
            worker: Arc::clone(&self.worker),
            handle: self.handle.take(),
            active: true,
        }
    }

    /// Arms this replacement ingester while retaining the exact closed process-publication gate.
    pub(in crate::cas_projection::connection) fn arm_for_publication(
        mut self,
        token: ProviderBrokerStartToken,
        startup_gate: &Arc<ServiceStartupGate>,
    ) -> Result<
        RunningProviderBrokerIngester,
        (StartBlockedProviderBrokerIngester, ProviderBrokerStartToken),
    > {
        let startup_guard = match startup_gate.lock_for_publication() {
            Ok(guard) => guard,
            Err(()) => return Err((self, token)),
        };
        if !Arc::ptr_eq(&self.startup_gate, startup_gate)
            || !Arc::ptr_eq(&self.control, &token.control)
        {
            return Err((self, token));
        }
        if let Err(token) = token.try_activate() {
            return Err((self, token));
        }
        drop(startup_guard);
        Ok(RunningProviderBrokerIngester {
            control: Arc::clone(&self.control),
            worker: Arc::clone(&self.worker),
            handle: self.handle.take(),
            active: true,
        })
    }

    pub(in crate::cas_projection::connection) fn cancel_and_join(
        mut self,
        token: ProviderBrokerStartToken,
    ) -> ProviderBrokerStopped {
        drop(token);
        let receipt = join_provider_ingester(
            self.handle.take(),
            self.control.home_id,
            self.control.commands.service_generation(),
            self.control.home_generation,
        );
        let joined = self.worker.take_joined_worker();
        ProviderBrokerStopped {
            worker: joined.worker,
            worker_disposition: joined.disposition,
            receipt,
        }
    }
}

impl RunningProviderBrokerIngester {
    pub(in crate::cas_projection::connection) fn arm_ordinary_worker_release(
        &self,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        self.worker.arm_ordinary_release()
    }

    pub(in crate::cas_projection::connection) fn arm_worker_retention_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        self.worker.arm_retain_for_adoption(cut)
    }

    pub(in crate::cas_projection::connection) fn request_cancel(&self) {
        self.control.request_cancel();
    }

    pub(in crate::cas_projection::connection) fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    pub(in crate::cas_projection::connection) fn stop_and_join(mut self) -> ProviderBrokerStopped {
        self.control.request_cancel();
        let receipt = join_provider_ingester(
            self.handle.take(),
            self.control.home_id,
            self.control.commands.service_generation(),
            self.control.home_generation,
        );
        self.active = false;
        let joined = self.worker.take_joined_worker();
        ProviderBrokerStopped {
            worker: joined.worker,
            worker_disposition: joined.disposition,
            receipt,
        }
    }
}

fn join_provider_ingester(
    handle: Option<JoinHandle<ProviderBrokerTerminalReceipt>>,
    _home_id: BerylHomeId,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
    home_generation: HomeGeneration,
) -> ProviderBrokerTerminalReceipt {
    let receipt =
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or(ProviderBrokerTerminalReceipt {
                service_generation,
                home_generation,
                clean: false,
            });
    #[cfg(test)]
    if take_forced_provider_broker_join_failure(_home_id, home_generation, service_generation) {
        return ProviderBrokerTerminalReceipt {
            clean: false,
            ..receipt
        };
    }
    receipt
}

#[cfg(test)]
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
fn take_forced_provider_broker_join_failure(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
) -> bool {
    let failure = ForcedProviderBrokerJoinFailure {
        home_id,
        home_generation,
        service_generation,
    };
    let Some(failures) = FORCED_PROVIDER_BROKER_JOIN_FAILURES.get() else {
        return false;
    };
    let mut failures = failures
        .lock()
        .expect("the provider-ingester join-failure test hook is usable");
    let Some(index) = failures.iter().position(|armed| *armed == failure) else {
        return false;
    };
    failures.swap_remove(index);
    true
}

impl Drop for RunningProviderBrokerIngester {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            self.control.request_cancel();
            drop(self.handle.take());
        }
    }
}

impl ProviderBrokerControl {
    pub(in crate::cas_projection) fn stop_coordinator(
        &self,
    ) -> Arc<crate::cas_projection::stop::StopCoordinator> {
        Arc::clone(&self.stop_coordinator)
    }

    pub(in crate::cas_projection) fn context_compaction_coordinator(
        &self,
    ) -> Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator> {
        Arc::clone(&self.context_compaction)
    }

    pub(in crate::cas_projection::connection) fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        #[cfg(feature = "test-faults")]
        {
            let broker_id = Arc::as_ptr(&self.cancelled) as usize;
            crate::cas_projection::test_faults::observe_approval_submit_cancellation(broker_id);
            crate::cas_projection::test_faults::observe_provider_stage_cancellation(
                crate::cas_projection::test_faults::ProviderTestKey::new(self.home_id, broker_id),
            );
        }
        self.approval.close();
        self.steering_results.close();
        self.ack.wake();
    }

    pub(in crate::cas_projection::connection) fn take_approval_interruption(
        &self,
    ) -> Option<PendingApprovalInterruption> {
        self.approval.take()
    }

    pub(in crate::cas_projection::connection) fn close_approval_interruptions(&self) {
        self.approval.close();
    }

    pub(super) fn close_checked_steering_lifecycles_for_loss(&self) {
        self.steering_results.close();
    }

    pub(in crate::cas_projection::connection) fn seal_checked_steering_proven_nondispatch(&self) {
        self.steering_results.seal_armed_after_proven_nondispatch();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn close_checked_steering_lifecycles_for_test(&self) {
        self.close_checked_steering_lifecycles_for_loss();
    }

    pub(in crate::cas_projection::connection) fn clear_approval_interruption(&self) {
        self.approval.clear_pending();
    }

    pub(in crate::cas_projection) fn arm_checked_steering_lifecycle(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        if !attempt.matches_target(
            route.input().thread_id(),
            route.target(),
            route.loaded_generation(),
        ) || attempt.home_generation_number() != home_generation.get()
        {
            return Err(CheckedSteeringLifecycleArmError::TargetMismatch);
        }
        self.steering_results
            .arm(route.clone(), home_generation, correlation.clone())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn arm_checked_steering_lifecycle_for_test(
        &self,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        self.steering_results
            .arm(route.clone(), home_generation, correlation.clone())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn take_checked_steering_lifecycle(
        &self,
    ) -> Option<CheckedSteeringLifecycle> {
        self.steering_results.take()
    }

    pub(in crate::cas_projection::connection) fn has_checked_steering_lifecycle(&self) -> bool {
        self.steering_results.has_ready()
    }

    pub(in crate::cas_projection::connection) fn wait_for_checked_steering_consumption(
        &self,
        timeout: std::time::Duration,
    ) {
        self.steering_results.wait_while_ready(timeout);
    }

    pub(in crate::cas_projection::connection) fn record_backend_failure(&self) {
        self.routing_failure
            .record(WholeConnectionRoutingFailure::Backend);
    }

    pub(in crate::cas_projection::connection) fn record_router_failure(&self) {
        self.routing_failure
            .record(WholeConnectionRoutingFailure::Router);
    }

    pub(in crate::cas_projection::connection) fn routing_failure(
        &self,
    ) -> Option<ConnectionRoutingFailure> {
        self.routing_failure.get().map(Into::into)
    }

    pub(in crate::cas_projection::connection) fn page_diagnostics(&self) -> PagePoolDiagnostics {
        self.pages.diagnostics()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection::connection) fn test_key(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderTestKey {
        crate::cas_projection::test_faults::ProviderTestKey::new(
            self.home_id,
            Arc::as_ptr(&self.cancelled) as usize,
        )
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection::connection) fn test_snapshot(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderBrokerSnapshot {
        self.test_metrics.snapshot()
    }
}

impl ProviderBrokerStopped {
    pub(in crate::cas_projection::connection) const fn receipt(
        &self,
    ) -> ProviderBrokerTerminalReceipt {
        self.receipt
    }

    pub(in crate::cas_projection::connection) fn into_worker(
        self,
    ) -> Option<ProjectionWorkerPermit> {
        self.worker
    }

    pub(in crate::cas_projection::connection) fn retains_worker(&self) -> bool {
        self.worker.is_some()
    }

    pub(in crate::cas_projection::connection) fn validate_receipt(
        &self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> Result<(), ProviderBrokerIngesterJoinError> {
        self.receipt
            .validate_exact(service_generation, home_generation)
    }

    pub(in crate::cas_projection::connection) fn into_adoption(
        mut self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<ProviderBrokerAdoptionStopped, Self> {
        if self.worker_disposition != Some(ProviderBrokerWorkerDisposition::RetainForAdoption(cut))
            || !self
                .receipt
                .is_exact(cut.service_generation, cut.home_generation)
        {
            return Err(self);
        }
        let Some(worker) = self.worker.take() else {
            return Err(self);
        };
        Ok(ProviderBrokerAdoptionStopped {
            worker,
            receipt: self.receipt,
            cut,
        })
    }
}

impl ProviderBrokerAdoptionStopped {
    pub(in crate::cas_projection::connection) const fn receipt(
        &self,
    ) -> ProviderBrokerTerminalReceipt {
        self.receipt
    }

    pub(in crate::cas_projection::connection) const fn cut_identity(
        &self,
    ) -> PersistentFailureCutIdentity {
        self.cut
    }

    pub(in crate::cas_projection::connection) fn into_worker(self) -> ProjectionWorkerPermit {
        self.worker
    }
}

impl ProviderBrokerTerminalReceipt {
    pub(in crate::cas_projection::connection) fn validate_exact(
        self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> Result<(), ProviderBrokerIngesterJoinError> {
        if !self.clean {
            return Err(ProviderBrokerIngesterJoinError::WorkerFailed);
        }
        if self.service_generation != service_generation || self.home_generation != home_generation
        {
            return Err(ProviderBrokerIngesterJoinError::EpochMismatch);
        }
        Ok(())
    }

    pub(in crate::cas_projection::connection) fn is_exact(
        self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> bool {
        self.validate_exact(service_generation, home_generation)
            .is_ok()
    }
}

impl std::fmt::Debug for ProviderBrokerControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderBrokerControl")
            .field("cancelled", &self.cancelled.load(Ordering::Acquire))
            .field("pages", &self.pages.diagnostics())
            .finish_non_exhaustive()
    }
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

impl Ingester {
    fn run(mut self) -> ProviderBrokerTerminalReceipt {
        let service_generation = self.commands.service_generation();
        let home_generation = self.home_generation;
        let clean = catch_unwind(AssertUnwindSafe(|| self.run_loop())).is_ok();
        ProviderBrokerTerminalReceipt {
            service_generation,
            home_generation,
            clean,
        }
    }

    fn cancel_before_start(mut self) -> ProviderBrokerTerminalReceipt {
        self.cancelled.store(true, Ordering::Release);
        self.abandon_active();
        self.approval.close();
        self.steering_results.close();
        self.ack.close();
        ProviderBrokerTerminalReceipt {
            service_generation: self.commands.service_generation(),
            home_generation: self.home_generation,
            clean: true,
        }
    }

    fn run_loop(&mut self) {
        let mut persistent_failure = false;
        while let Some(operation) = receive_next(&self.receiver, &self.cancelled) {
            let command = match self.commands.authorize() {
                Ok(command) => command,
                Err(_) => {
                    persistent_failure = self.commands.failure_observed();
                    self.cancelled.store(true, Ordering::Release);
                    let (reply, _) = self.apply(operation);
                    self.ack.complete_terminal(reply);
                    break;
                }
            };
            self.command = Some(command);
            let (reply, terminal) = self.apply(operation);
            if self.authority_lost {
                self.command
                    .take()
                    .expect("authority loss retains its outer live command")
                    .release_after_authority_loss();
                self.ack.complete_terminal(reply);
                self.cancelled.store(true, Ordering::Release);
                break;
            }
            persistent_failure = self.exact_persistent_failure();
            if terminal || persistent_failure {
                // The terminal acknowledgement may unblock an owner that immediately waits for
                // the persistent-failure cut. Release this operation's outer drain-counted permit
                // first; operation-specific nested permits have already settled inside `apply`.
                drop(self.command.take());
                self.ack.complete_terminal(reply);
                self.cancelled.store(true, Ordering::Release);
                break;
            }
            // Every acknowledgement is the public completion boundary for its operation. Release
            // the outer drain-counted permit before publishing that boundary so the submitter
            // cannot observe a completed operation that is still counted as live.
            drop(self.command.take());
            self.ack.complete(reply);
        }
        if !persistent_failure && !self.authority_lost {
            self.abandon_active();
        }
        self.approval.close();
        self.steering_results.close();
        self.ack.close();
    }

    fn apply(&mut self, operation: BrokerOperation) -> (BrokerReply, bool) {
        if self.cancelled.load(Ordering::Acquire) {
            let reply = match operation {
                BrokerOperation::Ordered(operation) => {
                    BrokerReply::Rejected(operation, OrderedTurnStreamSubmitCause::Cancelled)
                }
                BrokerOperation::SteeringSelect(selection) => {
                    BrokerReply::SteeringSelectionRejected(
                        selection,
                        OrderedTurnStreamSubmitCause::Cancelled,
                    )
                }
                BrokerOperation::SteeringChecked(message) => BrokerReply::SteeringCheckedRejected(
                    message,
                    OrderedTurnStreamSubmitCause::Cancelled,
                ),
                BrokerOperation::SteeringAbandon(_) => {
                    BrokerReply::SteeringAbandonRejected(OrderedTurnStreamSubmitCause::Cancelled)
                }
            };
            return (reply, true);
        }
        match operation {
            BrokerOperation::Ordered(operation) => match operation {
                OrderedTurnStreamOperation::ThreadStatusChanged(status) => {
                    self.thread_status_changed(status)
                }
                operation @ OrderedTurnStreamOperation::ThreadClosed(_) => (
                    BrokerReply::Rejected(
                        operation,
                        OrderedTurnStreamSubmitCause::Rejected(
                            OrderedTurnStreamRejection::InvalidControl,
                        ),
                    ),
                    true,
                ),
                OrderedTurnStreamOperation::TurnStarted(turn) => self.turn_started(turn),
                OrderedTurnStreamOperation::CheckedUserMessage(message) => {
                    self.checked_user_message(message)
                }
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal) => {
                    self.normal_turn_terminal(terminal)
                }
                OrderedTurnStreamOperation::Approval(request) => self.route_approval(request),
                OrderedTurnStreamOperation::DynamicBegin(call) => self.begin_dynamic(call),
                OrderedTurnStreamOperation::DynamicArgumentControl(control) => {
                    self.control_dynamic(control)
                }
                OrderedTurnStreamOperation::DynamicAcquirePage => self.acquire_dynamic_page(),
                OrderedTurnStreamOperation::DynamicArgumentFragment(fragment) => {
                    self.fragment_dynamic(fragment)
                }
                OrderedTurnStreamOperation::DynamicSeal => self.seal_dynamic(),
                OrderedTurnStreamOperation::DynamicAbandon(reason) => self.abandon_dynamic(reason),
                OrderedTurnStreamOperation::ProviderBegin(begin) => self.begin(begin),
                OrderedTurnStreamOperation::ProviderControl(control) => self.control(control),
                OrderedTurnStreamOperation::ProviderAcquirePage => self.acquire_page(),
                OrderedTurnStreamOperation::ProviderFragment(fragment) => self.fragment(fragment),
                OrderedTurnStreamOperation::ProviderSeal(route) => self.seal(route),
                OrderedTurnStreamOperation::ProviderAbandon(reason) => {
                    self.abandon_provider(reason)
                }
            },
            BrokerOperation::SteeringSelect(selection) => {
                self.select_steering_user_message(selection)
            }
            BrokerOperation::SteeringChecked(message) => {
                self.checked_steering_user_message(message)
            }
            BrokerOperation::SteeringAbandon(reason) => self.abandon_steering_user_message(reason),
        }
    }

    fn invalidate_target(&self, target: TargetInvalidation) -> TargetRouteOutcome {
        if self.exact_persistent_failure() {
            return TargetRouteOutcome::Terminal;
        }
        let reason = target.reason;
        if registry::invalidate_exact_generation(
            &target.key,
            self.authority.generation,
            target.owner,
            target.loaded_generation,
        )
        .is_err()
        {
            self.retire();
            return TargetRouteOutcome::Terminal;
        }
        if reason == LiveEventTargetCloseReason::ThreadClosed {
            TargetRouteOutcome::Applied
        } else {
            TargetRouteOutcome::TargetFailure
        }
    }

    fn retire(&self) {
        self.retire_for(
            WholeConnectionRoutingFailure::Router,
            LiveEventTargetCloseReason::StreamFailure,
        );
    }

    fn retire_for(
        &self,
        failure: WholeConnectionRoutingFailure,
        reason: LiveEventTargetCloseReason,
    ) {
        if self.exact_persistent_failure() {
            return;
        }
        self.record_failure(failure);
        let _ = self.authority.retire();
        self.router.retire(reason);
    }

    fn record_failure(&self, failure: WholeConnectionRoutingFailure) {
        self.routing_failure.record(failure);
    }

    fn provider_is_active(&self) -> bool {
        matches!(self.active, Some(ActiveIngress::Provider(_)))
    }

    fn take_provider(&mut self) -> Option<ActiveObservation> {
        match self.active.take() {
            Some(ActiveIngress::Provider(observation)) => Some(observation),
            other => {
                self.active = other;
                None
            }
        }
    }

    fn put_provider(&mut self, observation: ActiveObservation) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Provider(observation));
    }

    fn take_dynamic(&mut self) -> Option<ActiveDynamicTool> {
        match self.active.take() {
            Some(ActiveIngress::Dynamic(dynamic)) => Some(dynamic),
            other => {
                self.active = other;
                None
            }
        }
    }

    fn put_dynamic(&mut self, dynamic: ActiveDynamicTool) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Dynamic(dynamic));
    }

    fn take_steering(&mut self) -> Option<ActiveSteeringLifecycle> {
        match self.active.take() {
            Some(ActiveIngress::Steering(steering)) => Some(steering),
            other => {
                self.active = other;
                None
            }
        }
    }

    fn put_steering(&mut self, steering: ActiveSteeringLifecycle) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Steering(steering));
    }

    fn abandon_active(&mut self) {
        if self.exact_persistent_failure() {
            return;
        }
        match self.active.take() {
            Some(ActiveIngress::Provider(observation)) => observation.abandon(),
            Some(ActiveIngress::Dynamic(dynamic)) => {
                self.router.abandon_dynamic_tool(&dynamic.permit);
                drop(dynamic);
            }
            Some(ActiveIngress::Steering(steering)) => {
                self.steering_results.abandon_selection();
                let _ = steering.permit.fail();
            }
            None => {}
        }
    }

    fn current_observation_authority(&self) -> Option<(HomeGeneration, SyndicStorage)> {
        if self.home.home_id() != self.home_id {
            return None;
        }
        let before = self.home.health();
        if before.state() != HomeHealthState::Healthy {
            return None;
        }
        let generation = before.generation()?;
        let storage = SyndicStorage::reacquire(&self.home).ok()?;
        let after = self.home.health();
        (after.state() == HomeHealthState::Healthy && after.generation() == Some(generation))
            .then_some((generation, storage))
    }

    fn exact_persistent_failure(&self) -> bool {
        let health = self.home.health();
        let failed = self.home.home_id() == self.home_id
            && health.state() == HomeHealthState::Failed
            && health.generation() == Some(self.home_generation);
        if failed {
            let _ = self.failure_notification.notify();
        }
        self.commands.is_persistent_failure_cut()
    }

    fn authority_lost_terminal(&mut self) -> (BrokerReply, bool) {
        self.authority_lost = true;
        (
            BrokerReply::Applied(beryl_backend::OrderedTurnStreamCompletion::Applied),
            true,
        )
    }

    fn live_command(&self) -> &LiveCommandPermit {
        self.command
            .as_ref()
            .expect("provider apply retains its operation-scoped live command")
    }

    fn committer(
        &self,
        identity: ProviderObservationId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
    ) -> super::staging::StageCommitter<'_> {
        super::staging::StageCommitter {
            home: &self.home,
            home_id: self.home_id,
            home_generation,
            storage,
            identity,
            cancelled: &self.cancelled,
            command: self.live_command(),
            #[cfg(feature = "test-faults")]
            test_metrics: &self.test_metrics,
        }
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_ingester.rs"
    ));
}
