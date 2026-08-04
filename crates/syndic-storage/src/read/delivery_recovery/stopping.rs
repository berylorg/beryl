use beryl_model::{BindingRevision, InputGateRevision, SyndicExecutionSnapshotId};

use crate::{
    AcceptedRouteHeadProof, AcceptedRouteTarget, ActiveCasBinding, ActiveCasTurnRecord,
    BindingState, ExecutionSnapshotRecord, HistorySummaryRecord, InputGateRecord, InputGateState,
    NextTurnReason, StaleCasBinding, StopAbandonmentReason, StopAttemptNonce,
    StopCauseFirstRevisions, StopDispatchClaimWitness, StopOperationId, StopOperationRecord,
    StopOperationRevision, StopOperationState, SyndicCurrentBinding, SyndicRecordError,
    SyndicTimestamp, TurnLifecycle, TurnStateRevision,
};

use super::{
    DeliveryRecoveryClassificationError,
    classifier::{
        corruption, minimum_timestamp, required, selected_route, validate_blocking_turn,
        validate_current_tail,
    },
    facts,
};

pub(in crate::read) mod provider;

/// Exact current live stop authority shared by runtime coordination and startup recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicLiveStopOperation {
    record: StopOperationRecord,
    snapshot: ExecutionSnapshotRecord,
    current_gate_revision: InputGateRevision,
    current_state_revision: TurnStateRevision,
    stopped_route: Option<AcceptedRouteHeadProof>,
    minimum_timestamp: SyndicTimestamp,
}

impl SyndicLiveStopOperation {
    /// Returns the exact live retained record authenticated by recovery classification.
    #[must_use]
    pub const fn record(&self) -> &StopOperationRecord {
        &self.record
    }

    /// Returns the durable stop operation selected by the stopping gate.
    #[must_use]
    pub const fn operation_id(&self) -> StopOperationId {
        self.record.id()
    }

    /// Returns the exact immutable provider operation selected for interruption.
    #[must_use]
    pub const fn target(&self) -> &crate::StopOperationTarget {
        self.record.target()
    }

    /// Returns the current live stop-record revision.
    #[must_use]
    pub const fn stop_revision(&self) -> StopOperationRevision {
        self.record.revision()
    }

    /// Returns whether this stop was admitted or dispatch-claimed before restart.
    #[must_use]
    pub const fn state(&self) -> StopOperationState {
        self.record.state()
    }

    /// Returns the four exact persisted cause-first-publication revisions.
    #[must_use]
    pub const fn cause_first_revisions(&self) -> StopCauseFirstRevisions {
        self.record.cause_first_revisions()
    }

    /// Returns the exact retained dispatch-claim source and attempt, when any.
    #[must_use]
    pub const fn dispatch_claim(&self) -> Option<StopDispatchClaimWitness> {
        self.record.dispatch_claim()
    }

    /// Returns the retained attempt identity when dispatch may have crossed.
    #[must_use]
    pub const fn attempt(&self) -> Option<StopAttemptNonce> {
        self.record.attempt()
    }

    /// Returns the current stopping-gate revision.
    #[must_use]
    pub const fn current_gate_revision(&self) -> InputGateRevision {
        self.current_gate_revision
    }

    /// Returns the current active turn-state revision.
    #[must_use]
    pub const fn current_state_revision(&self) -> TurnStateRevision {
        self.current_state_revision
    }

    /// Returns the selected ordinary stopped route, or `None` for a provider operation.
    #[must_use]
    pub const fn stopped_route(&self) -> Option<AcceptedRouteHeadProof> {
        self.stopped_route
    }

    /// Returns the immutable execution snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot.id()
    }

    /// Returns the immutable execution snapshot authenticated against the current target.
    #[must_use]
    pub const fn snapshot(&self) -> &ExecutionSnapshotRecord {
        &self.snapshot
    }

    /// Returns the active binding revision that startup must retire.
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.snapshot.binding_revision()
    }

    /// Returns the lower timestamp bound for startup abandonment.
    #[must_use]
    pub const fn minimum_timestamp(&self) -> SyndicTimestamp {
        self.minimum_timestamp
    }

    /// Returns the closed reason required when process restart loses foreground dispatch authority.
    #[must_use]
    pub const fn startup_abandonment_reason(&self) -> StopAbandonmentReason {
        StopAbandonmentReason::StartupProcessGenerationLost
    }

    /// Builds stale-binding provenance for a later atomic startup-abandonment request.
    ///
    /// `observed_at` must be at least [`Self::minimum_timestamp`]. This method builds only bounded
    /// provenance; it neither consumes the stop nor authorizes backend dispatch.
    pub fn startup_stale_binding(
        &self,
        reason: impl AsRef<str>,
        observed_at: SyndicTimestamp,
    ) -> Result<StaleCasBinding, SyndicRecordError> {
        StaleCasBinding::new(
            self.snapshot.execution().clone(),
            self.snapshot.cas_thread_id().clone(),
            Some(self.snapshot.tool_profile()),
            Some(self.snapshot.represented_base_prefix()),
            Some(self.snapshot.lineage()),
            Some(self.snapshot.represented_base_native_turn_count()),
            Some(self.snapshot.loaded_generation()),
            reason,
            observed_at,
        )
    }
}

pub(super) fn classify(
    facts: &facts::RecoveryFacts,
    gate: &InputGateRecord,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
) -> Result<SyndicLiveStopOperation, DeliveryRecoveryClassificationError> {
    let InputGateState::Stopping {
        turn_id,
        operation_nonce,
    } = gate.state()
    else {
        return corruption("stop recovery classifier received a non-stopping gate");
    };
    let state = validate_blocking_turn(facts, *turn_id)?;
    validate_current_tail(state, summary)?;
    if !matches!(
        state.lifecycle(),
        TurnLifecycle::Pending | TurnLifecycle::Active
    ) {
        return corruption("stopping recovery turn is neither pending nor active");
    }
    let turn = required(
        facts.turn.as_ref(),
        "stopping recovery turn header is missing",
    )?;
    if turn.id() != *turn_id || turn.origin_thread_id() != gate.thread_id() {
        return corruption("stopping recovery turn header disagrees");
    }

    let record = required(
        facts.stop.as_ref(),
        "stopping gate has no matching stop-operation record",
    )?;
    let operation_id = StopOperationId::new(gate.thread_id(), *operation_nonce);
    if record.id() != operation_id || !record.state().is_live() {
        return corruption("stopping gate does not select one matching live stop operation");
    }
    let target = record.target();
    if target.thread_id() != gate.thread_id()
        || target.turn_id() != *turn_id
        || target.turn_kind() != turn.kind()
    {
        return corruption("stopping recovery stop target disagrees with its blocked turn");
    }

    let selected = gate
        .selected_route()
        .ok_or(DeliveryRecoveryClassificationError::Corruption(
            "stopping recovery gate has no selected stopped route",
        ))?;
    let route = selected_route(facts, gate)?;
    if selected != record.admission().successor_stopped_route()
        || record.admission().successor_gate_revision() > gate.revision()
        || route.target() != &AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
        || route.ready_retryable_count() != 0
        || route.delivering_count() != 0
        || route.delivering_logical_utf8_bytes() != 0
        || gate.live_steering_count() != 0
    {
        return corruption("stopping recovery route authority disagrees");
    }

    let BindingState::Active(active) = binding.binding().state() else {
        return corruption("stopping recovery has no current active binding");
    };
    let snapshot = required(
        facts.snapshot.as_ref(),
        "stopping recovery execution snapshot is missing",
    )?;
    validate_snapshot(gate, summary, binding, active, snapshot)?;
    let active_turn = required(
        facts.active_turn.as_ref(),
        "stopping recovery target has no published CAS turn",
    )?;
    validate_target(target, binding, snapshot, active_turn)?;

    let minimum = minimum_timestamp(state, summary)
        .max(turn.submitted_at())
        .max(snapshot.started_at())
        .max(active_turn.published_at());
    Ok(SyndicLiveStopOperation {
        record: record.clone(),
        snapshot: snapshot.clone(),
        current_gate_revision: gate.revision(),
        current_state_revision: state.revision(),
        stopped_route: Some(selected),
        minimum_timestamp: minimum,
    })
}

fn validate_snapshot(
    gate: &InputGateRecord,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
    active: &ActiveCasBinding,
    snapshot: &ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if snapshot.id() != active.snapshot_id()
        || snapshot.thread_id() != gate.thread_id()
        || snapshot.binding_revision() != binding.binding().revision()
        || snapshot.activation_gate_revision() != active.activation_gate_revision()
        || snapshot.activation_gate_revision() > gate.revision()
        || snapshot.active_turn_id() != active.turn_id()
        || snapshot.selected_path().tail() != Some(active.turn_id())
        || snapshot.selected_path().digest() != summary.selected_path_digest()
        || snapshot.cas_thread_id() != active.usable().cas_thread_id()
        || snapshot.selected_path() != binding.binding().selected_path()
        || snapshot.execution() != active.usable().execution()
        || snapshot.represented_base_prefix() != active.usable().represented_prefix()
        || snapshot.represented_base_native_turn_count() != active.usable().native_turn_count()
        || snapshot.tool_profile() != active.usable().tool_profile()
        || snapshot.lineage() != active.usable().lineage()
        || snapshot.started_at() != active.started_at()
    {
        return corruption("stopping recovery execution snapshot authority disagrees");
    }
    Ok(())
}

fn validate_target(
    target: &crate::StopOperationTarget,
    binding: &SyndicCurrentBinding,
    snapshot: &ExecutionSnapshotRecord,
    active_turn: &ActiveCasTurnRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if target.binding_revision() != binding.binding().revision()
        || target.snapshot_id() != snapshot.id()
        || target.runtime_id() != snapshot.execution().runtime_id()
        || target.loaded_generation() != snapshot.loaded_generation()
        || target.cas_thread_id() != snapshot.cas_thread_id()
        || target.cas_turn_id() != active_turn.cas_turn_id()
        || active_turn.snapshot_id() != snapshot.id()
        || active_turn.thread_id() != target.thread_id()
        || active_turn.turn_id() != target.turn_id()
        || active_turn.binding_revision() != target.binding_revision()
        || active_turn.cas_thread_id() != target.cas_thread_id()
        || active_turn.published_at() < snapshot.started_at()
    {
        return corruption("stopping recovery immutable target authority disagrees");
    }
    Ok(())
}
