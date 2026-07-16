use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::AtomicUsize,
        mpsc::{Receiver, SyncSender},
    },
};

use beryl_backend::{DynamicToolCallRequest, TurnStreamEvent};
use beryl_model::{
    CasLoadedSessionGeneration, CasThreadId, CasTurnId, DynamicToolCallId, RuntimeId,
    SyndicThreadId,
};
use thiserror::Error;

use super::{ProjectionConnection, registry::LoadedThreadKey};
use crate::cas_projection::{LoadedCasProjection, ProjectionCoordinatorError};

mod classify;
mod command;
mod consumer;
mod process;
mod snapshot;
mod state;
mod target;
#[cfg(test)]
mod tests;

pub use process::{LiveEventConnectionFact, LiveEventProcessSnapshot};

/// Maximum number of normalized live events retained for one active target.
pub const LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT: usize = 256;

/// Maximum approximate parsed bytes retained for one active target.
pub const LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT: usize = 16 * 1024 * 1024;

/// Maximum retired CAS-thread lanes remembered by one live connection.
const RETIRED_THREAD_LANE_LIMIT: usize = 256;

/// Why one target or its owning connection stopped accepting live events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEventTargetCloseReason {
    /// Explicit local connection retirement revoked all targets.
    ConnectionRetired,
    /// The normalized backend stream failed.
    StreamFailure,
    /// CAS emitted an explicit protocol-error event.
    ProtocolError,
    /// A required CAS thread, turn, item, or tool-call identity was invalid.
    InvalidEventIdentity,
    /// One target observed a CAS turn other than its one-way bound identity.
    ConflictingTurnIdentity,
    /// A response did not match an exact dynamic-tool request routed to this target.
    ConflictingDynamicToolIdentity,
    /// A turn-local event arrived before a provisional target observed turn start.
    EventBeforeTurnStart,
    /// An event arrived after the exact target turn completed.
    EventAfterTurnCompletion,
    /// The same provisional target attempted more than one `turn/start` dispatch.
    DuplicateTurnStart,
    /// The target's record or retained-byte queue bound was exceeded.
    QueueOverflow,
    /// The sole target receiver disappeared before event delivery.
    ReceiverAbandoned,
    /// CAS explicitly closed the target thread.
    ThreadClosed,
    /// The sole connection worker stopped.
    WorkerStopped,
    /// Remembering another retired remote thread lane would exceed the bounded fence.
    RetiredThreadLaneCapacity,
}

/// Current authority state of one exact connection-owned event router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEventConnectionState {
    /// The router still accepts normalized events.
    Active,
    /// The router is terminally retired for the supplied reason.
    Retired(LiveEventTargetCloseReason),
}

/// One normalized event routed to an exact CAS thread and optional turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutedLiveEvent {
    thread_id: CasThreadId,
    turn_id: Option<CasTurnId>,
    event: Box<TurnStreamEvent>,
    approximate_retained_bytes: usize,
}

impl RoutedLiveEvent {
    /// Returns the validated CAS thread identity used for routing.
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    /// Returns the validated CAS turn identity when the event is turn-local.
    #[must_use]
    pub const fn turn_id(&self) -> Option<&CasTurnId> {
        self.turn_id.as_ref()
    }

    /// Borrows the normalized backend event.
    #[must_use]
    pub const fn event(&self) -> &TurnStreamEvent {
        &self.event
    }

    /// Returns the approximate parsed bytes charged to the target queue.
    #[must_use]
    pub const fn approximate_retained_bytes(&self) -> usize {
        self.approximate_retained_bytes
    }

    /// Consumes the envelope and returns its normalized backend event.
    #[must_use]
    pub fn into_event(self) -> TurnStreamEvent {
        *self.event
    }
}

/// Result of one bounded wait on an exact live-event target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiveEventPoll {
    /// One exact routed event was available.
    Event(RoutedLiveEvent),
    /// The target remains live but no event arrived before the timeout.
    Quiet,
    /// The target is terminally closed.
    Closed(LiveEventTargetCloseReason),
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
    queued_event_count: usize,
    queued_event_bytes: usize,
    routed_event_count: u64,
    unmatched_event_count: u64,
    rejected_event_count: u64,
    overflow_count: u64,
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

    /// Returns the total queued records across registered targets.
    #[must_use]
    pub const fn queued_event_count(&self) -> usize {
        self.queued_event_count
    }

    /// Returns the total approximate parsed bytes across registered target queues.
    #[must_use]
    pub const fn queued_event_bytes(&self) -> usize {
        self.queued_event_bytes
    }

    /// Returns the number of events accepted by exact targets.
    #[must_use]
    pub const fn routed_event_count(&self) -> u64 {
        self.routed_event_count
    }

    /// Returns the number of valid thread events with no exact target.
    #[must_use]
    pub const fn unmatched_event_count(&self) -> u64 {
        self.unmatched_event_count
    }

    /// Returns the number of events rejected for invalid identity.
    #[must_use]
    pub const fn rejected_event_count(&self) -> u64 {
        self.rejected_event_count
    }

    /// Returns the number of target queues closed for overflow.
    #[must_use]
    pub const fn overflow_count(&self) -> u64 {
        self.overflow_count
    }

    /// Returns the number of idle worker polls that observed no event.
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
#[derive(Clone, Debug, Error, Eq, PartialEq)]
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
    /// The router could not allocate another nonzero target generation.
    #[error("the live-event target generation is exhausted")]
    GenerationExhausted,
    /// The router state lock was poisoned.
    #[error("the live-event router mutex is poisoned")]
    RouterPoisoned,
    /// Exact loaded-projection registry validation failed.
    #[error(transparent)]
    ProjectionRegistry(#[from] ProjectionCoordinatorError),
}

/// Failure to bind or operate one already registered live-event target.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LiveEventTargetError {
    /// The target no longer exists in its owning router.
    #[error("the live-event target is no longer registered")]
    TargetClosed,
    /// The target had already bound a different CAS turn.
    #[error("the live-event target observed CAS turn {actual} instead of {expected}")]
    ConflictingTurnIdentity {
        /// CAS turn supplied by the confirming caller.
        expected: CasTurnId,
        /// Different CAS turn already observed by the target.
        actual: CasTurnId,
    },
    /// The connection retired while closing this target fail-closed.
    #[error("the connection retired while closing the live-event target")]
    ConnectionRetired,
    /// The router state lock was poisoned.
    #[error("the live-event router mutex is poisoned")]
    RouterPoisoned,
}

#[derive(Debug)]
struct QueuedLiveEvent {
    event: RoutedLiveEvent,
    retained_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetTurn {
    AwaitingStart,
    Exact,
    Terminal,
}

#[derive(Debug)]
struct TargetEntry {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    turn_state: TargetTurn,
    turn_id: Option<CasTurnId>,
    start_dispatched: bool,
    dynamic_tool_requests: HashMap<DynamicToolCallId, DynamicToolCallRequest>,
    sender: SyncSender<QueuedLiveEvent>,
    queued_count: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
    terminal: Arc<Mutex<Option<LiveEventTargetCloseReason>>>,
}

#[derive(Debug)]
struct RouterState {
    revision: u64,
    next_registration: u64,
    retired: Option<LiveEventTargetCloseReason>,
    targets: HashMap<CasThreadId, TargetEntry>,
    retired_thread_lanes: HashSet<CasThreadId>,
    routed_event_count: u64,
    unmatched_event_count: u64,
    rejected_event_count: u64,
    overflow_count: u64,
    quiet_poll_count: u64,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct EventRouter {
    runtime_id: RuntimeId,
    process_generation: beryl_model::CasProcessGeneration,
    connection_generation: u64,
    process: Arc<process::ProcessEventProjection>,
    state: Mutex<RouterState>,
}

impl Drop for EventRouter {
    fn drop(&mut self) {
        self.process.retire_connection(
            self.connection_generation,
            LiveEventTargetCloseReason::WorkerStopped,
        );
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) struct TargetRegistration {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    receiver: Receiver<QueuedLiveEvent>,
    queued_count: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
    terminal: Arc<Mutex<Option<LiveEventTargetCloseReason>>>,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct TargetRegistrationProof {
    registration: u64,
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    terminal: Arc<Mutex<Option<LiveEventTargetCloseReason>>>,
}

impl TargetRegistrationProof {
    fn terminal_reason(&self) -> Option<LiveEventTargetCloseReason> {
        self.terminal.lock().ok().and_then(|terminal| *terminal)
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

    pub(in crate::cas_projection) fn terminal_reason(&self) -> Option<LiveEventTargetCloseReason> {
        self.terminal.lock().ok().and_then(|terminal| *terminal)
    }

    pub(in crate::cas_projection) fn proof(&self) -> TargetRegistrationProof {
        TargetRegistrationProof {
            registration: self.registration,
            key: self.key.clone(),
            owner: self.owner,
            loaded_generation: self.loaded_generation,
            terminal: Arc::clone(&self.terminal),
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
    ProvenTerminal,
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum LiveEventTargetHandoffError {
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
    #[error("the target has not observed its exact terminal turn event")]
    TargetNotTerminal,
    #[error("the target still owns {count} queued events retaining {bytes} bytes")]
    QueuedEvents { count: usize, bytes: usize },
    #[error("the target still owns unresolved dynamic-tool requests")]
    DynamicToolResponsesPending,
    #[error("the live-event router mutex is poisoned")]
    RouterPoisoned,
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
