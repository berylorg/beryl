pub(super) mod classifier;
pub(super) mod facts;
mod pages;
pub(super) mod stopping;

pub use stopping::SyndicLiveStopOperation;

use std::fmt;

use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasThreadId, DomainRevision,
    InputGateRevision, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};

use crate::{
    AbandonActiveBinding, AcceptedRouteGeneration, AcceptedRouteLostTarget,
    ExecutionSnapshotRecord, InputGateRecord, InputGateState, StaleCasBinding, SyndicReadError,
    SyndicRecordError, SyndicTimestamp, TurnStateRevision,
};

/// Maximum physical input-gate rows scanned by one delivery-recovery page.
pub const DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS: usize = 256;

/// Maximum stored or practical decoded input-gate bytes scanned by one recovery page.
pub const DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES: usize = 65_536;

/// Same-home continuation after the last physical input-gate thread key scanned at startup.
///
/// This cursor deliberately carries no domain revision. The exclusive startup owner may mutate
/// already visited gates without invalidating forward progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryRecoveryStartupCursor {
    home_id: BerylHomeId,
    after_thread_id: SyndicThreadId,
}

/// One compact non-idle input-gate row discovered by startup recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRecoverySource {
    home_id: BerylHomeId,
    gate: InputGateRecord,
}

impl DeliveryRecoverySource {
    /// Returns the owning Syndic thread.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.gate.thread_id()
    }

    /// Returns the gate revision observed by the startup scan.
    #[must_use]
    pub const fn gate_revision(&self) -> InputGateRevision {
        self.gate.revision()
    }

    /// Returns the non-idle gate state observed by the startup scan.
    #[must_use]
    pub const fn gate_state(&self) -> &InputGateState {
        self.gate.state()
    }

    /// Returns the blocking turn carried by this non-idle source row.
    #[must_use]
    pub fn turn_id(&self) -> SyndicTurnId {
        self.gate
            .state()
            .blocking_turn_id()
            .expect("delivery-recovery startup pages exclude idle gates")
    }
}

/// One bounded startup page over physical input-gate order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRecoveryStartupPage {
    records: Vec<DeliveryRecoverySource>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<DeliveryRecoveryStartupCursor>,
}

impl DeliveryRecoveryStartupPage {
    /// Returns non-idle source rows in stable thread-key order.
    #[must_use]
    pub fn records(&self) -> &[DeliveryRecoverySource] {
        &self.records
    }

    /// Returns aggregate stored bytes for all physical gate rows scanned by this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns aggregate practical decoded bytes for all physical gate rows scanned by this page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Returns the continuation after the last physical row, even when filtering returned no rows.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<DeliveryRecoveryStartupCursor> {
        self.next_cursor
    }
}

/// Domain-revision-bound continuation after the last physical gate row scanned for pending work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveredPendingCursor {
    home_id: BerylHomeId,
    source_revision: DomainRevision,
    after_thread_id: SyndicThreadId,
}

impl RecoveredPendingCursor {
    /// Returns the domain revision fencing this continuation.
    #[must_use]
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }
}

/// Compact proof that one existing turn is safe undispatched pending work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveredPendingSource {
    source_revision: DomainRevision,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    gate_revision: InputGateRevision,
    state_revision: TurnStateRevision,
    minimum_timestamp: SyndicTimestamp,
}

impl RecoveredPendingSource {
    /// Returns the domain revision fencing all facts in this source.
    #[must_use]
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }

    /// Returns the owning thread.
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    /// Returns the existing pending turn that must execute without another promotion.
    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }

    /// Returns the exact safe-pending gate revision.
    #[must_use]
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }

    /// Returns the exact source-free pending turn-state revision.
    #[must_use]
    pub const fn state_revision(self) -> TurnStateRevision {
        self.state_revision
    }

    /// Returns the lower timestamp bound from turn-state and thread-history activity.
    #[must_use]
    pub const fn minimum_timestamp(self) -> SyndicTimestamp {
        self.minimum_timestamp
    }
}

/// One bounded revision-fenced page of proven safe recovered pending turns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredPendingPage {
    source_revision: DomainRevision,
    records: Vec<RecoveredPendingSource>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<RecoveredPendingCursor>,
}

impl RecoveredPendingPage {
    /// Returns the exact domain revision fencing this page and its continuation.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        self.source_revision
    }

    /// Returns proven safe pending rows in stable thread-key order.
    #[must_use]
    pub fn records(&self) -> &[RecoveredPendingSource] {
        &self.records
    }

    /// Returns stored bytes for every physical gate row scanned, including filtered rows.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns practical decoded bytes for every physical gate row scanned.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Returns progress after the last physical row even when no safe pending row was emitted.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RecoveredPendingCursor> {
        self.next_cursor
    }
}

/// Exact durable active authority that startup must abandon before scheduling opens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveDeliveryRecovery {
    snapshot: ExecutionSnapshotRecord,
    current_gate_revision: InputGateRevision,
    current_state_revision: TurnStateRevision,
    route_generation: AcceptedRouteGeneration,
    lost_target: AcceptedRouteLostTarget,
    minimum_timestamp: SyndicTimestamp,
}

impl ActiveDeliveryRecovery {
    /// Returns the owning Syndic thread.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.snapshot.thread_id()
    }

    /// Returns the possibly dispatched turn.
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.snapshot.active_turn_id()
    }

    /// Returns the active binding revision to use as pending-activation authority.
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.snapshot.binding_revision()
    }

    /// Returns the gate revision established by activation.
    ///
    /// This is the `PendingTurnActivation` gate revision, not a later steering or admission
    /// descendant.
    #[must_use]
    pub const fn gate_revision(&self) -> InputGateRevision {
        self.snapshot.activation_gate_revision()
    }

    /// Returns the current descendant gate revision observed by classification.
    #[must_use]
    pub const fn current_gate_revision(&self) -> InputGateRevision {
        self.current_gate_revision
    }

    /// Returns the pending state revision that preceded the first activation event.
    #[must_use]
    pub const fn state_revision(&self) -> TurnStateRevision {
        TurnStateRevision::FIRST
    }

    /// Returns the current turn-state revision observed by classification.
    #[must_use]
    pub const fn current_state_revision(&self) -> TurnStateRevision {
        self.current_state_revision
    }

    /// Returns the immutable execution snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot.id()
    }

    /// Returns the CAS thread whose active binding must be retired.
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        self.snapshot.cas_thread_id()
    }

    /// Returns the loaded generation captured before possible dispatch.
    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.snapshot.loaded_generation()
    }

    /// Returns the timestamp originally persisted by binding activation.
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.snapshot.started_at()
    }

    /// Returns the lower timestamp bound for abandonment followed by terminal publication.
    #[must_use]
    pub const fn minimum_timestamp(&self) -> SyndicTimestamp {
        self.minimum_timestamp
    }

    /// Builds the exact generic projection-loss abandonment for this stabilized authority.
    ///
    /// `observed_at` must be at least [`Self::minimum_timestamp`].
    pub fn generic_abandonment(
        &self,
        reason: impl AsRef<str>,
        observed_at: SyndicTimestamp,
    ) -> Result<AbandonActiveBinding, SyndicRecordError> {
        let stale = StaleCasBinding::new(
            self.snapshot.execution().clone(),
            self.snapshot.cas_thread_id().clone(),
            Some(self.snapshot.tool_profile()),
            Some(self.snapshot.represented_base_prefix()),
            Some(self.snapshot.lineage()),
            Some(self.snapshot.represented_base_native_turn_count()),
            Some(self.snapshot.loaded_generation()),
            reason,
            observed_at,
        )?;
        Ok(AbandonActiveBinding::new(
            self.snapshot.thread_id(),
            self.snapshot.binding_revision(),
            self.route_generation,
            self.lost_target.clone(),
            self.snapshot.selected_path(),
            stale,
        ))
    }
}

/// Closed stabilized restart classification for one startup source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryRecoveryCase {
    /// Existing ordinary work is proven not to have crossed the dispatch boundary.
    Pending {
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        minimum_timestamp: SyndicTimestamp,
    },
    /// Durable activation is possible-dispatch authority and must be abandoned generically.
    Active(Box<ActiveDeliveryRecovery>),
    /// Exact live stop authority must be abandoned without replaying backend dispatch.
    Stopping(Box<SyndicLiveStopOperation>),
    /// Active abandonment committed and only source-less terminal convergence remains.
    PostAbandonment {
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        minimum_timestamp: SyndicTimestamp,
    },
    /// Terminal publication committed, but bounded canonical/transcript convergence remains.
    FinalizingHistory {
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        minimum_timestamp: SyndicTimestamp,
    },
    /// Compaction recovery is intentionally deferred to its owning phase.
    DeferredCompaction {
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    },
    /// The source's thread is already idle and needs no restart action.
    Settled { thread_id: SyndicThreadId },
}

/// Why one fixed-work delivery-recovery classification could not be published.
#[derive(Debug)]
pub enum DeliveryRecoveryClassificationError {
    /// A bounded storage acquisition failed.
    Read(SyndicReadError),
    /// The startup source or one of its stabilized dependent records changed.
    SourceDrift,
    /// The stabilized records form a coherent but unsupported durable combination.
    Corruption(&'static str),
}

impl fmt::Display for DeliveryRecoveryClassificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::SourceDrift => {
                formatter.write_str("delivery-recovery source changed during classification")
            }
            Self::Corruption(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for DeliveryRecoveryClassificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::SourceDrift | Self::Corruption(_) => None,
        }
    }
}

impl From<SyndicReadError> for DeliveryRecoveryClassificationError {
    fn from(source: SyndicReadError) -> Self {
        match source {
            SyndicReadError::ConcurrentChange { .. } => Self::SourceDrift,
            SyndicReadError::Invariant(message) => Self::Corruption(message),
            source => Self::Read(source),
        }
    }
}
