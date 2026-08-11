use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use beryl_backend::{
    FreshIdleThread, ManagedBackendError, ManagedBackendSession, ThreadInjectionOutcome,
    ThreadInjectionPreflight, ThreadInjectionSourceError, ThreadInjectionSourcePage,
    ThreadUnsubscribeStatus,
};
use beryl_home_store::HomeStore;
use beryl_model::{
    BerylHomeId, CasLoadedSessionGeneration, CasProcessGeneration, CasThreadId, RuntimeId,
    SyndicThreadId,
};
use beryl_stream::PageLease;
use thiserror::Error;

use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionExecutionError,
    stop::{
        StopCoordinationError, StopCoordinationOutcome, StopCoordinator, StopDispatchOwner,
        StopDispatchSettlement, StopOwnership,
    },
};

mod attachment;
mod authority;
mod driver;
mod driver_outcome;
mod forwarding_hub;
mod lease;
mod lifecycle;
mod persistent_failure;
mod provider_broker;
mod recovery_source_broker;
pub(super) mod registry;
mod router;
mod source_broker;
mod target_command;
mod thread_closed;

pub use recovery_source_broker::{
    RecoveryReplayCapacityDiagnostics, RecoveryReplayDiagnosticsObserver,
    RecoveryReplayDiagnosticsSnapshot,
};
pub use router::{
    LiveEventConnectionFact, LiveEventConnectionState, LiveEventPoll, LiveEventProcessSnapshot,
    LiveEventRouterSnapshot, LiveEventTarget, LiveEventTargetCloseReason,
    LiveEventTargetRegistrationError, RoutedApproval, RoutedDynamicToolCall,
};

use attachment::ConnectionAttachment;
pub(in crate::cas_projection) use attachment::ConnectionAttachmentIdentity;
use driver::ConnectionDriver;
pub(in crate::cas_projection) use driver::{
    ConnectionRequestSession, ExactContextCompactionDispatch,
};
pub(in crate::cas_projection) use driver_outcome::{
    ConnectionCommandOutcome, ConnectionRoutingFailure,
};
use forwarding_hub::{ForwardingAttachmentEndpoint, ForwardingHub};
use lease::RawLoadedLeaseSeed;
pub(super) use lease::{
    ExistingLease, LoadedProjectionLease, LocalLoadedRegistryDispositionOwner,
    TerminalLoadedLeaseDispositionOwner, ThreadRetirement,
};
pub(in crate::cas_projection) use lifecycle::ProjectionConnectionIdentityObservation;
pub(in crate::cas_projection) use persistent_failure::{
    PersistentFailureCompletedTarget, PersistentFailureCompletion, PersistentFailureDriverResult,
    PersistentFailureInterruptDisposition, PersistentFailureNoDispatchReason,
};
#[cfg(test)]
pub(in crate::cas_projection) use provider_broker::CheckedSteeringLifecycle;
pub(in crate::cas_projection) use provider_broker::{
    ActiveBindingLossDisposition, CheckedSteeringLifecycleArmError, CheckedSteeringLifecycleOwner,
    CheckedSteeringLifecycleWaitError, ProviderBrokerLossError, ProviderBrokerLossOutcome,
};
use provider_broker::{ProviderBroker, ProviderBrokerControl};
pub(in crate::cas_projection) use router::EventRouter;
pub(in crate::cas_projection) use router::LiveEventTargetHandoffError;
#[cfg(test)]
pub(in crate::cas_projection) use router::PersistentFailureTargetIneligibility;
pub(in crate::cas_projection) use router::PersistentFailureTargetWitness;
pub(in crate::cas_projection) use router::TargetRegistrationProof;
pub(in crate::cas_projection) use router::TargetTurnRegistration;
pub(in crate::cas_projection) use router::{
    ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptFinishError,
    ActiveSteeringAttemptFinishOutcome, ActiveSteeringAttemptPermit,
    ActiveSteeringTargetLookupError, StopElectionAcquireError, StopElectionAdmission,
    StopElectionPermit, StopTargetProof, TargetAuthorizationFailure,
};
use router::{ConnectionProcessFact, TargetRegistration};
pub(in crate::cas_projection) use router::{
    LiveEventTargetLossError, LiveEventTargetLossOutcome, ProvenTerminalOutcome,
};
pub(in crate::cas_projection) use source_broker::StreamedInputBrokerService;
pub(in crate::cas_projection) use target_command::turn_start_allows_not_started;
pub(in crate::cas_projection) use target_command::{
    TargetTurnStartActivationFailure, TargetTurnStartOutcome,
};
pub(in crate::cas_projection) use thread_closed::{
    ConnectionThreadClosedOutcome, record_connection_thread_closed,
};

use crate::cas_projection::service_config::ProjectionWorkerPermitPair;
pub(in crate::cas_projection) use registry::LoadedThreadKey;
use registry::{
    ConnectionGeneration, ExistingSubscription, LeaseToken, ObservedSubscription,
    allocate_connection_generation,
};

pub(super) use authority::{
    ConnectionCleanupOwner, ConnectionPromotionReleaseOutcome, ConnectionPromotionReservation,
    ConnectionRegistryAuthority, ConnectionRetirementOutcome,
};
pub(super) use lifecycle::ProjectionConnection;

/// Non-authorizing outcome of consuming one loaded-projection lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadedProjectionReleaseOutcome {
    /// Another exact local lease still owns the same CAS subscription.
    SharedSubscriptionRemains,
    /// The exact local lease was already revoked by a broader invalidation.
    AlreadyRevoked,
    /// CAS classified the exact connection-scoped unsubscribe request.
    Unsubscribe(ThreadUnsubscribeStatus),
    /// Local authority was revoked and the whole connection was retired.
    ConnectionRetired,
}

/// Failure after local authority for a consumed projection lease was revoked.
#[derive(Debug, Error)]
pub enum LoadedProjectionReleaseError {
    #[error("loaded-projection registry could not revoke the exact lease")]
    Registry(#[source] ProjectionCoordinatorError),
    #[error("thread/unsubscribe failed after local projection authority was revoked")]
    Backend(#[source] Box<ManagedBackendError>),
    #[error("live-event target {thread_id} closed while thread/unsubscribe completed: {reason:?}")]
    LiveEventRouting {
        thread_id: CasThreadId,
        reason: LiveEventTargetCloseReason,
    },
}
