use beryl_model::DraftRevision;

use super::{DraftSaveRequest, DraftSaveToken};

/// Proven non-mutating reason one save did not commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftKnownUnchanged {
    /// The domain rejected the update before mutation assembly.
    ValidationRejected,
    /// Cancellation was observed before writer admission.
    CancelledBeforeAdmission,
}

/// Reason publication is suspended until an exact current-draft reread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSuspensionCause {
    /// Another admitted command changed an expected revision.
    RevisionConflict,
    /// A post-admission storage result cannot prove the old or new state.
    AmbiguousStorageFailure,
    /// A purported success carried an impossible durable revision.
    InvalidCommitRevision,
}

/// Closed diagnostic status carried by an opaque executor-issued completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSaveOutcome {
    /// The exact requested payload passed the durability barrier.
    Committed { revision: DraftRevision },
    /// The durable draft is proven unchanged by this request.
    KnownUnchanged(DraftKnownUnchanged),
    /// Current durable state must be reread before publication may resume.
    RequiresReconciliation(DraftSuspensionCause),
}

/// Result of asking the service to start a timed autosave.
#[derive(Clone, Debug)]
pub enum DraftAutosaveAction {
    Clean,
    NotDue,
    InFlight(DraftSaveToken),
    Suspended(DraftSuspensionCause),
    Started(DraftSaveRequest),
}

/// Result of requesting a lifecycle flush.
#[derive(Clone, Debug)]
pub enum DraftFlushAction {
    Complete,
    Waiting(DraftSaveToken),
    Suspended(DraftSuspensionCause),
    Started(DraftSaveRequest),
}

/// State-machine effect of one asynchronous completion.
#[derive(Clone, Debug)]
pub enum DraftCompletionAction {
    Stale,
    Published {
        flush_complete: bool,
    },
    Chained(DraftSaveRequest),
    KnownUnchanged {
        reason: DraftKnownUnchanged,
        flush_failed: bool,
    },
    Suspended(DraftSuspensionCause),
}

/// State-machine effect after an exact same-home reconciliation seed.
#[derive(Clone, Debug)]
pub enum DraftReconciliationAction {
    Ready,
    FlushComplete,
    Chained(DraftSaveRequest),
}
