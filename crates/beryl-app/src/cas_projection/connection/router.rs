use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize},
        mpsc::{Receiver, SyncSender},
    },
};

use beryl_backend::{ApprovalInterruption, ApprovalRequest, DynamicToolCall};
use beryl_model::{
    CasItemId, CasLoadedSessionGeneration, CasThreadId, CasTurnId, DynamicToolCallId, RuntimeId,
    SyndicThreadId,
};
use syndic_storage::{SyndicTimestamp, TurnEndStatus};
use thiserror::Error;

use super::{ProjectionConnection, registry::LoadedThreadKey};
use crate::cas_projection::{
    LiveCommandAuthorizer, LoadedCasProjection, PendingTurnActivation, ProjectionCoordinatorError,
};

mod activation;
mod active_steering;
mod approval;
mod close_reason;
mod command;
mod compaction;
mod consumer;
mod delayed_steering;
mod dynamic_tool;
mod loss;
mod permit;
mod persistent_failure;
mod process;
mod registration;
mod snapshot;
mod state;
mod stop;
mod target;
#[cfg(test)]
mod tests;

pub(in crate::cas_projection::connection) use activation::ResponseActivationProofError;
pub(in crate::cas_projection) use activation::TargetTurnRegistration;
pub(in crate::cas_projection) use active_steering::{
    ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptFinishError,
    ActiveSteeringAttemptFinishOutcome, ActiveSteeringAttemptPermit, ActiveSteeringAttemptStatus,
    ActiveSteeringTargetLookupError,
};
pub(in crate::cas_projection) use approval::{
    ApprovalInterruptionObligation, ApprovalRouteOutcome, PreparedApprovalInterruption,
};
pub use close_reason::LiveEventTargetCloseReason;
pub(in crate::cas_projection) use compaction::{
    CompactionControlPermit, CompactionControlPermitError,
};
pub(in crate::cas_projection) use delayed_steering::{
    DelayedSteeringLifecycleFinishError, DelayedSteeringLifecyclePermit,
    DelayedSteeringLifecyclePermitError,
};
pub(in crate::cas_projection::connection) use dynamic_tool::{
    DynamicToolResponseAuthorization, DynamicToolTargetError, DynamicToolTargetPermit,
};
pub(in crate::cas_projection) use loss::{
    TargetLossAcquisition, TargetLossFinishError, TargetLossPublicationAuthority,
    TargetLossRequestError,
};
pub(in crate::cas_projection) use permit::{
    SourcePublicationFinishError, SourcePublicationPermit, SourcePublicationPermitError,
};
#[cfg(test)]
pub(in crate::cas_projection) use persistent_failure::PersistentFailureTargetCandidate;
pub(in crate::cas_projection) use persistent_failure::{
    PersistentFailureTargetBatch, PersistentFailureTargetGuardDisposition,
    PersistentFailureTargetGuardObservation, PersistentFailureTargetGuardSettlementError,
    PersistentFailureTargetIneligibility, PersistentFailureTargetProof,
    PersistentFailureTargetWitness,
};
pub use process::{LiveEventConnectionFact, LiveEventProcessSnapshot};
pub(in crate::cas_projection::connection) use process::{
    ProcessEventObservation, StableConnectionProcessFact,
};
pub(in crate::cas_projection::connection) use state::TargetProjectionDropSettlement;
pub(in crate::cas_projection) use stop::{
    StopElectionAcquireError, StopElectionPermit, StopTargetProof,
};

/// Maximum number of feature-owned approval and dynamic-tool operations retained per target.
const TARGET_OPERATION_QUEUE_CAPACITY: usize = 256;

/// Maximum retired CAS-thread lanes remembered by one live connection.
const RETIRED_THREAD_LANE_LIMIT: usize = 256;

/// Maximum exact live targets retained by one mounted connection router.
const LIVE_EVENT_TARGET_CAPACITY: usize = 64;

/// Current authority state of one exact connection-owned event router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEventConnectionState {
    /// The router still accepts exact target operations and publication permits.
    Active,
    /// The router is terminally retired for the supplied reason.
    Retired(LiveEventTargetCloseReason),
}

/// One compact approval routed to its exact registered CAS turn.
///
/// The envelope keeps the sole non-cloneable backend request. Its route was validated atomically
/// with queue admission before the ordered broker acknowledged delivery. It is presentation-only;
/// any separately required permission interruption is owned by the connection driver.
pub struct RoutedApproval {
    request: ApprovalRequest,
    interruption: ApprovalInterruption,
}

impl std::fmt::Debug for RoutedApproval {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoutedApproval")
            .field("thread_id", &self.request.thread_id())
            .field("turn_id", &self.request.turn_id())
            .field("item_id", &self.request.item_id())
            .field("interruption", &self.interruption)
            .finish()
    }
}

/// One exact routed dynamic-tool call with its sole backend response owner and typed request.
pub struct RoutedDynamicToolCall {
    response: RoutedDynamicToolResponse,
    request: crate::conversation_tools::RoutedDynamicToolRequest,
}

pub(in crate::cas_projection) struct RoutedDynamicToolResponse {
    call: DynamicToolCall,
    authorization: DynamicToolResponseAuthorization,
}

pub(in crate::cas_projection) struct DynamicToolResponseWrite {
    call: DynamicToolCall,
    _admission: Arc<dynamic_tool::DynamicToolResponseAdmission>,
}

impl std::fmt::Debug for RoutedDynamicToolCall {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoutedDynamicToolCall")
            .field("call", &self.response.call)
            .field(
                "rejected",
                &matches!(
                    &self.request,
                    crate::conversation_tools::RoutedDynamicToolRequest::Rejected(_)
                ),
            )
            .finish()
    }
}

impl RoutedApproval {
    /// Returns the exact validated CAS thread identity.
    #[must_use]
    pub fn thread_id(&self) -> &CasThreadId {
        self.request
            .thread_id()
            .expect("routed approval retains its validated thread identity")
    }

    /// Returns the exact validated CAS turn identity.
    #[must_use]
    pub fn turn_id(&self) -> &CasTurnId {
        self.request
            .turn_id()
            .expect("routed approval retains its validated turn identity")
    }

    /// Returns the optional validated CAS item identity.
    #[must_use]
    pub fn item_id(&self) -> Option<&CasItemId> {
        self.request.item_id()
    }

    /// Returns whether routing admitted a separate exact driver-owned interruption.
    #[must_use]
    pub fn interruption(&self) -> ApprovalInterruption {
        self.interruption.clone()
    }

    /// Borrows the compact backend approval request.
    #[must_use]
    pub const fn request(&self) -> &ApprovalRequest {
        &self.request
    }

    /// Consumes the routed approval into its sole request and interruption disposition.
    #[must_use]
    pub fn into_parts(self) -> (ApprovalRequest, ApprovalInterruption) {
        let Self {
            request,
            interruption,
        } = self;
        (request, interruption)
    }
}

impl RoutedDynamicToolCall {
    /// Returns the exact CAS thread validated before argument ingress.
    #[must_use]
    pub fn thread_id(&self) -> &CasThreadId {
        self.response.call.thread_id()
    }

    /// Returns the exact CAS turn validated before argument ingress.
    #[must_use]
    pub fn turn_id(&self) -> &CasTurnId {
        self.response.call.turn_id()
    }

    /// Returns the exact compact call identity reserved before argument ingress.
    #[must_use]
    pub fn call_id(&self) -> &DynamicToolCallId {
        self.response.call.call_id()
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        RoutedDynamicToolResponse,
        crate::conversation_tools::RoutedDynamicToolRequest,
    ) {
        (self.response, self.request)
    }
}

impl RoutedDynamicToolResponse {
    pub(super) fn into_parts(self) -> (DynamicToolResponseAuthorization, DynamicToolResponseWrite) {
        let admission = Arc::clone(&self.authorization.admission);
        (
            self.authorization,
            DynamicToolResponseWrite {
                call: self.call,
                _admission: admission,
            },
        )
    }
}

impl DynamicToolResponseWrite {
    pub(super) const fn call(&self) -> &DynamicToolCall {
        &self.call
    }
}

/// Result of one bounded wait on an exact live target.
#[derive(Debug)]
pub enum LiveEventPoll {
    /// One exact compact approval presentation was available; interruption is driver-owned.
    Approval(RoutedApproval),
    /// One sealed dynamic-tool call with a feature-owned typed request or rejection.
    DynamicTool(RoutedDynamicToolCall),
    /// The exact durable terminal outcome is available after all earlier queued operations.
    ProvenTerminal(ProvenTerminalOutcome),
    /// The target remains live but no operation arrived before the timeout.
    Quiet,
    /// The target is terminally closed.
    Closed(LiveEventTargetCloseReason),
}

/// One exact status-only terminal publication exposed after its durable commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvenTerminalOutcome {
    status: TurnEndStatus,
    observed_at: SyndicTimestamp,
    same_native_reacquisition_required: bool,
}

impl ProvenTerminalOutcome {
    #[cfg(test)]
    pub(in crate::cas_projection) const fn new(
        status: TurnEndStatus,
        observed_at: SyndicTimestamp,
    ) -> Self {
        Self {
            status,
            observed_at,
            same_native_reacquisition_required: false,
        }
    }

    pub(in crate::cas_projection) const fn new_with_reacquisition(
        status: TurnEndStatus,
        observed_at: SyndicTimestamp,
        same_native_reacquisition_required: bool,
    ) -> Self {
        Self {
            status,
            observed_at,
            same_native_reacquisition_required,
        }
    }

    /// Returns the exact durable provider and captured-history outcome.
    #[must_use]
    pub const fn status(self) -> TurnEndStatus {
        self.status
    }

    /// Returns the timestamp carried by the durable terminal source event.
    #[must_use]
    pub const fn observed_at(self) -> SyndicTimestamp {
        self.observed_at
    }

    /// Returns whether an exact durable narrative mismatch requires same-native reacquisition.
    #[must_use]
    pub const fn same_native_reacquisition_required(self) -> bool {
        self.same_native_reacquisition_required
    }
}

/// Bounded content-free diagnostics for one connection-owned event router.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveEventRouterSnapshot {
    runtime_id: RuntimeId,
    process_generation: beryl_model::CasProcessGeneration,
    connection_generation: u64,
    revision: u64,
    state: LiveEventConnectionState,
    target_count: usize,
    queued_operation_count: usize,
    outstanding_dynamic_tool_count: usize,
    routed_operation_count: u64,
    unmatched_operation_count: u64,
    rejected_operation_count: u64,
    queue_pressure_count: u64,
    quiet_poll_count: u64,
    retired_thread_lane_count: usize,
}

impl LiveEventRouterSnapshot {
    /// Returns the exact configured runtime.
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    /// Returns the exact managed-process generation.
    #[must_use]
    pub const fn process_generation(&self) -> beryl_model::CasProcessGeneration {
        self.process_generation
    }

    /// Returns the exact admitted-connection generation.
    #[must_use]
    pub const fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    /// Returns the monotonic router-fact revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns whether this router remains active.
    #[must_use]
    pub const fn state(&self) -> LiveEventConnectionState {
        self.state
    }

    /// Returns the number of registered per-thread targets.
    #[must_use]
    pub const fn target_count(&self) -> usize {
        self.target_count
    }

    /// Returns the exact number of queued approval and dynamic-tool operations.
    #[must_use]
    pub const fn queued_operation_count(&self) -> usize {
        self.queued_operation_count
    }

    /// Returns the number of calls still holding exact response authority.
    #[must_use]
    pub const fn outstanding_dynamic_tool_count(&self) -> usize {
        self.outstanding_dynamic_tool_count
    }

    /// Returns the number of feature-owned operations accepted by exact targets.
    #[must_use]
    pub const fn routed_operation_count(&self) -> u64 {
        self.routed_operation_count
    }

    /// Returns the number of valid operations with no exact target.
    #[must_use]
    pub const fn unmatched_operation_count(&self) -> u64 {
        self.unmatched_operation_count
    }

    /// Returns the number of operations rejected for invalid identity or authority.
    #[must_use]
    pub const fn rejected_operation_count(&self) -> u64 {
        self.rejected_operation_count
    }

    /// Returns the number of exact target-operation queues closed under pressure.
    #[must_use]
    pub const fn queue_pressure_count(&self) -> u64 {
        self.queue_pressure_count
    }

    /// Returns the number of idle worker polls that observed no operation.
    #[must_use]
    pub const fn quiet_poll_count(&self) -> u64 {
        self.quiet_poll_count
    }

    /// Returns the number of remote thread identities fenced from reuse.
    #[must_use]
    pub const fn retired_thread_lane_count(&self) -> usize {
        self.retired_thread_lane_count
    }
}

/// Failure to consume a loaded projection into its sole live-event target.
#[derive(Debug, Error)]
pub enum LiveEventTargetRegistrationError {
    /// The exact loaded projection lease was already revoked.
    #[error("the exact loaded CAS projection is no longer live")]
    ProjectionNotLive,
    /// The owning connection router is already retired.
    #[error("the connection live-event router is retired")]
    ConnectionRetired,
    /// The CAS thread already owns another exact target.
    #[error("CAS thread {thread_id} already has an active live-event target")]
    TargetAlreadyRegistered {
        /// Exact CAS thread that already owns a target.
        thread_id: CasThreadId,
    },
    /// A prior target for this CAS thread retired on the same connection.
    #[error("CAS thread {thread_id} cannot be rebound after its prior live-event target retired")]
    TargetGenerationRetired {
        /// Exact remote thread identity fenced from generation-blind reuse.
        thread_id: CasThreadId,
    },
    /// The connection already owns its complete bounded live-target set.
    #[error("the live-event target capacity {capacity} is full")]
    TargetCapacityFull {
        /// Configured target capacity of the exact connection router.
        capacity: usize,
    },
    /// The pending activation belongs to a different durable Syndic owner.
    #[error("the pending turn activation does not belong to the target's Syndic owner")]
    ActivationAuthorityMismatch,
    /// The router could not allocate another nonzero target generation.
    #[error("the live-event target generation is exhausted")]
    GenerationExhausted,
    /// The router state lock was poisoned.
    #[error("the live-event router mutex is poisoned")]
    RouterPoisoned,
    /// Exact loaded-projection registry validation failed.
    #[error(transparent)]
    ProjectionRegistry(#[from] ProjectionCoordinatorError),
    /// Broker-owned rollback could not retire the active binding after registration failed.
    #[error("the failed provisional target registration could not roll back binding activation")]
    ActivationCleanupFailed,
}

#[derive(Debug)]
struct QueuedTargetOperation {
    operation: RoutedTargetOperation,
}

#[derive(Debug)]
enum RoutedTargetOperation {
    Approval(RoutedApproval),
    DynamicTool(RoutedDynamicToolCall),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetTurn {
    AwaitingStart,
    AwaitingCompactionTurn,
    Exact,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetTerminalSignal {
    Open,
    Proven(ProvenTerminalOutcome),
    Closed(LiveEventTargetCloseReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetPublication {
    Source,
    CompactionControl,
    DelayedSteering,
    Loss,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveSteeringAttemptKey {
    token: u64,
    thread_id: CasThreadId,
    registration: u64,
    command_dispatched: bool,
    loss_transferred: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveStopElectionKey {
    token: u64,
    thread_id: CasThreadId,
    registration: u64,
}

#[derive(Debug)]
struct TargetEntry {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    request_timeout: std::time::Duration,
    turn_state: TargetTurn,
    turn_id: Option<CasTurnId>,
    start_dispatched: bool,
    activation_durable: bool,
    pending_activation: Option<PendingTurnActivation>,
    compaction: Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority>,
    dynamic_tool_responses:
        HashMap<DynamicToolCallId, Arc<dynamic_tool::DynamicToolResponseAdmission>>,
    sender: Option<SyncSender<QueuedTargetOperation>>,
    queued_operations: Arc<AtomicUsize>,
    terminal: Arc<Mutex<TargetTerminalSignal>>,
    bound_turn: Arc<Mutex<Option<CasTurnId>>>,
    publication_in_flight: Option<TargetPublication>,
    publication_closing: Option<LiveEventTargetCloseReason>,
    loss_requested: bool,
    loss_receipt: Arc<AtomicBool>,
    persistent_failure_projection: Option<LoadedCasProjection>,
}

#[derive(Debug)]
struct RouterState {
    revision: u64,
    next_registration: u64,
    next_steering_attempt: u64,
    active_steering_attempt: Option<ActiveSteeringAttemptKey>,
    active_steering_attempt_waiter: bool,
    next_stop_election: u64,
    active_stop_election: Option<ActiveStopElectionKey>,
    persistent_failure: Option<persistent_failure::PersistentFailureRouterCut>,
    retired: Option<LiveEventTargetCloseReason>,
    targets: HashMap<CasThreadId, TargetEntry>,
    retired_thread_lanes: HashSet<CasThreadId>,
    routed_operation_count: u64,
    unmatched_operation_count: u64,
    rejected_operation_count: u64,
    queue_pressure_count: u64,
    quiet_poll_count: u64,
}

/// Result of settling one long-lived router authority against the master command cut.
pub(super) enum LongLivedAuthoritySettlement<T> {
    /// The authority settled while ordinary router cleanup was still authoritative.
    Settled(T),
    /// Persistent failure won first, so the exact authority remains in router state for freeze.
    PreservedForPersistentFailure,
    /// Router or gate synchronization was unavailable.
    Unavailable,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct AcceptedNextReadyNotifier {
    signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
}

impl AcceptedNextReadyNotifier {
    pub(in crate::cas_projection) fn notify(&self) {
        self.signal.wake(
            crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::AcceptedNextReady,
        );
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) struct EventRouter {
    runtime_id: RuntimeId,
    process_generation: beryl_model::CasProcessGeneration,
    connection_generation: u64,
    process: process::ProcessEventObservation,
    commands: LiveCommandAuthorizer,
    projection_retainer:
        Option<crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer>,
    state: Mutex<RouterState>,
    publication_changed: Condvar,
    scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
    #[cfg(test)]
    stop_election_wait_observer: Mutex<Option<SyncSender<()>>>,
    #[cfg(test)]
    terminal_publication_wait_observer: Mutex<Option<SyncSender<()>>>,
}

impl EventRouter {
    pub(super) fn failure_observed(&self) -> bool {
        self.commands.failure_observed()
    }

    pub(in crate::cas_projection) fn projection_retainer(
        &self,
    ) -> Result<
        crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
        ProjectionCoordinatorError,
    > {
        self.projection_retainer.clone().ok_or(
            ProjectionCoordinatorError::ProjectionConnectionUnavailable {
                runtime_id: self.runtime_id,
                process_generation: self.process_generation,
            },
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_state_for_test(self: &Arc<Self>) {
        let router = Arc::clone(self);
        assert!(
            std::thread::spawn(move || {
                let _state = router.state.lock().unwrap();
                panic!("poison the bounded live-event router state");
            })
            .join()
            .is_err()
        );
    }

    fn accepted_next_ready_notifier(&self) -> AcceptedNextReadyNotifier {
        AcceptedNextReadyNotifier {
            signal: self.scheduler_signal.clone(),
        }
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) struct TargetRegistration {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    receiver: Receiver<QueuedTargetOperation>,
    queued_operations: Arc<AtomicUsize>,
    terminal: Arc<Mutex<TargetTerminalSignal>>,
    loss_receipt: Arc<AtomicBool>,
    compaction: Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority>,
    #[cfg(test)]
    bound_turn: Arc<Mutex<Option<CasTurnId>>>,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct TargetRegistrationProof {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    terminal: Arc<Mutex<TargetTerminalSignal>>,
    loss_receipt: Arc<AtomicBool>,
    compaction: Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority>,
}

impl TargetRegistrationProof {
    pub(in crate::cas_projection) fn key(&self) -> &LoadedThreadKey {
        &self.key
    }

    pub(in crate::cas_projection) const fn registration(&self) -> u64 {
        self.registration
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) const fn compaction(
        &self,
    ) -> Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority> {
        self.compaction
    }

    pub(in crate::cas_projection) fn terminal_reason(&self) -> Option<LiveEventTargetCloseReason> {
        self.terminal
            .lock()
            .ok()
            .and_then(|terminal| match *terminal {
                TargetTerminalSignal::Closed(reason) => Some(reason),
                TargetTerminalSignal::Open | TargetTerminalSignal::Proven(_) => None,
            })
    }

    fn loss_was_published(&self) -> bool {
        self.loss_receipt.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl TargetRegistration {
    pub(in crate::cas_projection) fn key(&self) -> &LoadedThreadKey {
        &self.key
    }

    pub(in crate::cas_projection) fn owner(&self) -> SyndicThreadId {
        self.owner
    }

    pub(in crate::cas_projection) fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) const fn registration(&self) -> u64 {
        self.registration
    }

    pub(in crate::cas_projection) const fn compaction(
        &self,
    ) -> Option<crate::cas_projection::context_compaction::ContextCompactionTargetAuthority> {
        self.compaction
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn bound_turn(&self) -> Option<CasTurnId> {
        self.bound_turn.lock().ok().and_then(|turn| turn.clone())
    }

    pub(in crate::cas_projection) fn terminal_reason(&self) -> Option<LiveEventTargetCloseReason> {
        self.terminal
            .lock()
            .ok()
            .and_then(|terminal| match *terminal {
                TargetTerminalSignal::Closed(reason) => Some(reason),
                TargetTerminalSignal::Open | TargetTerminalSignal::Proven(_) => None,
            })
    }

    pub(in crate::cas_projection) fn proven_terminal(&self) -> Option<ProvenTerminalOutcome> {
        self.terminal
            .lock()
            .ok()
            .and_then(|terminal| match *terminal {
                TargetTerminalSignal::Proven(outcome) => Some(outcome),
                TargetTerminalSignal::Open | TargetTerminalSignal::Closed(_) => None,
            })
    }

    pub(in crate::cas_projection) fn proof(&self) -> TargetRegistrationProof {
        TargetRegistrationProof {
            registration: self.registration,
            key: self.key.clone(),
            owner: self.owner,
            loaded_generation: self.loaded_generation,
            terminal: Arc::clone(&self.terminal),
            loss_receipt: Arc::clone(&self.loss_receipt),
            compaction: self.compaction,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct TargetInvalidation {
    pub(in crate::cas_projection) key: LoadedThreadKey,
    pub(in crate::cas_projection) owner: SyndicThreadId,
    pub(in crate::cas_projection) loaded_generation: CasLoadedSessionGeneration,
    pub(in crate::cas_projection) reason: LiveEventTargetCloseReason,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) enum RouteOutcome {
    Continue,
    InvalidateTarget(TargetInvalidation),
    RetireConnection(LiveEventTargetCloseReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum TargetAuthorizationFailure {
    Target(LiveEventTargetCloseReason),
    Router,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum TargetHandoffRequirement {
    NotStarted,
    CompactionNotDispatched,
    ProvenTerminal,
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum LiveEventTargetHandoffError {
    #[error("pre-activation surrender authority is unavailable")]
    PreactivationSurrenderUnavailable {
        #[source]
        source: ProjectionCoordinatorError,
    },
    #[error("the turn-start outcome does not authorize a not-started handoff")]
    TurnStartOutcomeNotReusable,
    #[error("the turn-start outcome belongs to another live-event target")]
    TurnStartOutcomeTargetMismatch,
    #[error("the live-event target is no longer registered")]
    TargetClosed,
    #[error("the live-event connection is retired")]
    ConnectionRetired,
    #[error("the target has not remained in the required not-started state")]
    TargetMayHaveStarted,
    #[error("the target has not entered its exact terminal state")]
    TargetNotTerminal,
    #[error("the target has no exact durable terminal outcome")]
    TerminalOutcomeUnavailable,
    #[error("the target still owns {count} queued feature operations")]
    QueuedOperations { count: usize },
    #[error("the target still owns unresolved dynamic-tool requests")]
    DynamicToolResponsesPending,
    #[error("the live-event router mutex is poisoned")]
    RouterPoisoned,
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum LiveEventTargetLossError {
    #[error("broker-owned target-loss convergence failed")]
    Broker {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

#[allow(
    clippy::large_enum_variant,
    reason = "the sole exact target remains fixed-size and must cross the loss handoff without extra allocation"
)]
pub(in crate::cas_projection) enum LiveEventTargetLossOutcome {
    Incomplete,
    ProvenTerminal {
        target: LiveEventTarget,
        outcome: ProvenTerminalOutcome,
    },
}

/// Non-cloneable consumer for one exact loaded projection's live events.
///
/// Dropping the target unregisters its receiver and revokes the corresponding
/// loaded-session generation so later events cannot be reassigned.
#[derive(Debug)]
pub struct LiveEventTarget {
    projection: Option<LoadedCasProjection>,
    connection: Arc<ProjectionConnection>,
    registration: Option<TargetRegistration>,
}
