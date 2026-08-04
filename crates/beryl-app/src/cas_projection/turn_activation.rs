use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, InputGateRevision, SyndicExecutionSnapshotId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    CasTurnSource, LiveSourceEvent, PublishActiveCasTurn, SourceEventPayload, SourceEventSequence,
    SyndicTimestamp, TurnStateRevision,
};

/// Immutable durable authority carried by a provisional target before `turn/start` dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingTurnActivation {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    binding_revision: BindingRevision,
    gate_revision: InputGateRevision,
    state_revision: TurnStateRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    observed_at: SyndicTimestamp,
}

impl PendingTurnActivation {
    /// Creates the exact publication inputs established by durable binding activation.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        binding_revision: BindingRevision,
        gate_revision: InputGateRevision,
        state_revision: TurnStateRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        observed_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            binding_revision,
            gate_revision,
            state_revision,
            snapshot_id,
            observed_at,
        }
    }

    /// Returns the durable Syndic thread owning this activation.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    /// Returns the pending Syndic turn receiving the CAS source.
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    /// Returns the active binding revision established before dispatch.
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }

    /// Returns the pre-publication input-gate revision.
    #[must_use]
    pub const fn gate_revision(&self) -> InputGateRevision {
        self.gate_revision
    }

    /// Returns the pending turn-state revision preceding activation.
    #[must_use]
    pub const fn state_revision(&self) -> TurnStateRevision {
        self.state_revision
    }

    /// Returns the active execution snapshot receiving the CAS turn.
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    /// Returns the common non-regressing publication timestamp.
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }

    pub(crate) fn active_turn(
        &self,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
    ) -> PublishActiveCasTurn {
        PublishActiveCasTurn::new(
            self.thread_id,
            self.binding_revision,
            self.gate_revision,
            self.snapshot_id,
            cas_thread_id,
            cas_turn_id,
            self.observed_at,
        )
    }

    pub(crate) fn activation_event(
        &self,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        published_gate_revision: InputGateRevision,
    ) -> Result<LiveSourceEvent, syndic_storage::SyndicRecordError> {
        debug_assert_eq!(
            self.gate_revision.checked_next().ok(),
            Some(published_gate_revision)
        );
        LiveSourceEvent::new(
            self.thread_id,
            self.turn_id,
            self.state_revision,
            published_gate_revision,
            SourceEventSequence::new(1).expect("first source-event sequence is nonzero"),
            Some(CasTurnSource::new(cas_thread_id, cas_turn_id)),
            SourceEventPayload::TurnActivated,
            self.observed_at,
        )
    }
}
