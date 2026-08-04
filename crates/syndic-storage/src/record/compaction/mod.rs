use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasThreadId, CasTurnId,
    InputGateRevision, RuntimeId, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};

use crate::{
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionMarkerObservation,
    CompactionOperationId, CompactionOperationRevision, CompactionProviderSequence,
    CompactionRequestDisposition, CompactionSettlement, CompactionSettlementReceiptCommitment,
    CompactionThreadStatus, StopOperationNonce, TurnEndStatus, TurnStateRevision,
};

mod settlement;
mod stop_provenance;
mod transitions;

pub use settlement::*;

/// Immutable loaded provider target captured by compaction admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionOperationTarget {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    snapshot_id: SyndicExecutionSnapshotId,
    binding_revision: BindingRevision,
    runtime_id: RuntimeId,
    loaded_generation: CasLoadedSessionGeneration,
    cas_thread_id: CasThreadId,
}

impl CompactionOperationTarget {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        snapshot_id: SyndicExecutionSnapshotId,
        binding_revision: BindingRevision,
        runtime_id: RuntimeId,
        loaded_generation: CasLoadedSessionGeneration,
        cas_thread_id: CasThreadId,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            snapshot_id,
            binding_revision,
            runtime_id,
            loaded_generation,
            cas_thread_id,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
}

/// Immutable provenance for the sole compact-start dispatch claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionDispatchClaimWitness {
    source_revision: CompactionOperationRevision,
    attempt: CompactionAttemptNonce,
}

impl CompactionDispatchClaimWitness {
    #[must_use]
    pub const fn new(
        source_revision: CompactionOperationRevision,
        attempt: CompactionAttemptNonce,
    ) -> Self {
        Self {
            source_revision,
            attempt,
        }
    }
    #[must_use]
    pub const fn source_revision(self) -> CompactionOperationRevision {
        self.source_revision
    }
    #[must_use]
    pub const fn attempt(self) -> CompactionAttemptNonce {
        self.attempt
    }
}

/// One independently ordered compact-start response observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionRequestObservation {
    revision: CompactionOperationRevision,
    disposition: CompactionRequestDisposition,
}

impl CompactionRequestObservation {
    #[must_use]
    pub const fn new(
        revision: CompactionOperationRevision,
        disposition: CompactionRequestDisposition,
    ) -> Self {
        Self {
            revision,
            disposition,
        }
    }
    #[must_use]
    pub const fn revision(self) -> CompactionOperationRevision {
        self.revision
    }
    #[must_use]
    pub const fn disposition(self) -> CompactionRequestDisposition {
        self.disposition
    }
}

/// One exact normalized thread-status observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionStatusObservation {
    sequence: CompactionProviderSequence,
    status: CompactionThreadStatus,
}

impl CompactionStatusObservation {
    #[must_use]
    pub const fn new(sequence: CompactionProviderSequence, status: CompactionThreadStatus) -> Self {
        Self { sequence, status }
    }
    #[must_use]
    pub const fn sequence(self) -> CompactionProviderSequence {
        self.sequence
    }
    #[must_use]
    pub const fn status(self) -> CompactionThreadStatus {
        self.status
    }
}

/// One-way CAS-turn publication in the operation's provider order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionCasTurnObservation {
    sequence: CompactionProviderSequence,
    cas_turn_id: CasTurnId,
}

impl CompactionCasTurnObservation {
    #[must_use]
    pub const fn new(sequence: CompactionProviderSequence, cas_turn_id: CasTurnId) -> Self {
        Self {
            sequence,
            cas_turn_id,
        }
    }
    #[must_use]
    pub const fn sequence(&self) -> CompactionProviderSequence {
        self.sequence
    }
    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
}

/// Exact terminal evidence retained before bounded provider-item finalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionTerminalObservation {
    sequence: CompactionProviderSequence,
    status: TurnEndStatus,
    turn_state_revision: TurnStateRevision,
}

impl CompactionTerminalObservation {
    #[must_use]
    pub const fn new(
        sequence: CompactionProviderSequence,
        status: TurnEndStatus,
        turn_state_revision: TurnStateRevision,
    ) -> Self {
        Self {
            sequence,
            status,
            turn_state_revision,
        }
    }
    #[must_use]
    pub const fn sequence(self) -> CompactionProviderSequence {
        self.sequence
    }
    #[must_use]
    pub const fn status(self) -> TurnEndStatus {
        self.status
    }
    #[must_use]
    pub const fn turn_state_revision(self) -> TurnStateRevision {
        self.turn_state_revision
    }
}

/// Exact inert successor retained after one settlement wins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionConsumedWitness {
    source_revision: CompactionOperationRevision,
    successor_gate_revision: InputGateRevision,
    settlement: CompactionSettlement,
    receipt_commitment: CompactionSettlementReceiptCommitment,
}

impl CompactionConsumedWitness {
    #[must_use]
    pub const fn new(
        source_revision: CompactionOperationRevision,
        successor_gate_revision: InputGateRevision,
        settlement: CompactionSettlement,
        receipt_commitment: CompactionSettlementReceiptCommitment,
    ) -> Self {
        Self {
            source_revision,
            successor_gate_revision,
            settlement,
            receipt_commitment,
        }
    }
    #[must_use]
    pub const fn source_revision(&self) -> CompactionOperationRevision {
        self.source_revision
    }
    #[must_use]
    pub const fn successor_gate_revision(&self) -> InputGateRevision {
        self.successor_gate_revision
    }
    #[must_use]
    pub const fn settlement(&self) -> &CompactionSettlement {
        &self.settlement
    }
    #[must_use]
    pub const fn receipt_commitment(&self) -> CompactionSettlementReceiptCommitment {
        self.receipt_commitment
    }
}

/// Closed live, stop-handoff, finalization, or consumed state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionOperationState {
    Admitted,
    DispatchClaimed,
    Live,
    Stopping(StopOperationNonce),
    Finalizing,
    Consumed(CompactionConsumedWitness),
}

impl CompactionOperationState {
    #[must_use]
    pub const fn is_live(&self) -> bool {
        matches!(
            self,
            Self::Admitted | Self::DispatchClaimed | Self::Live | Self::Finalizing
        )
    }
}

/// Retained bounded V1 context-compaction operation authority and receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionOperationRecord {
    id: CompactionOperationId,
    home_id: BerylHomeId,
    target: CompactionOperationTarget,
    revision: CompactionOperationRevision,
    attempt: CompactionAttemptNonce,
    dispatch_claim: Option<CompactionDispatchClaimWitness>,
    request: Option<CompactionRequestObservation>,
    provider_frontier: Option<CompactionProviderSequence>,
    status: Option<CompactionStatusObservation>,
    cas_turn: Option<CompactionCasTurnObservation>,
    marker: Option<CompactionMarkerObservation>,
    terminal: Option<CompactionTerminalObservation>,
    state: CompactionOperationState,
}

impl CompactionOperationRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: CompactionOperationId,
        home_id: BerylHomeId,
        target: CompactionOperationTarget,
        revision: CompactionOperationRevision,
        attempt: CompactionAttemptNonce,
        dispatch_claim: Option<CompactionDispatchClaimWitness>,
        request: Option<CompactionRequestObservation>,
        provider_frontier: Option<CompactionProviderSequence>,
        status: Option<CompactionStatusObservation>,
        cas_turn: Option<CompactionCasTurnObservation>,
        marker: Option<CompactionMarkerObservation>,
        terminal: Option<CompactionTerminalObservation>,
        state: CompactionOperationState,
    ) -> Result<Self, crate::SyndicRecordError> {
        if id.thread_id() != target.thread_id()
            || id.provider_turn_id() != target.turn_id()
            || dispatch_claim.is_some_and(|claim| claim.attempt() != attempt)
            || request.is_some() && dispatch_claim.is_none()
            || provider_frontier.is_some() && dispatch_claim.is_none()
            || status.is_some_and(|value| Some(value.sequence()) > provider_frontier)
            || cas_turn
                .as_ref()
                .is_some_and(|value| Some(value.sequence()) > provider_frontier)
            || marker
                .as_ref()
                .is_some_and(|value| Some(value.sequence()) > provider_frontier)
            || terminal.is_some_and(|value| Some(value.sequence()) > provider_frontier)
            || marker.as_ref().is_some_and(|value| {
                value.lifecycle() == CompactionMarkerLifecycle::Completed && cas_turn.is_none()
            })
            || matches!(state, CompactionOperationState::Finalizing) && terminal.is_none()
            || matches!(state, CompactionOperationState::Stopping(_)) && cas_turn.is_none()
        {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        Ok(Self {
            id,
            home_id,
            target,
            revision,
            attempt,
            dispatch_claim,
            request,
            provider_frontier,
            status,
            cas_turn,
            marker,
            terminal,
            state,
        })
    }

    #[must_use]
    pub const fn id(&self) -> CompactionOperationId {
        self.id
    }
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    #[must_use]
    pub const fn target(&self) -> &CompactionOperationTarget {
        &self.target
    }
    #[must_use]
    pub const fn revision(&self) -> CompactionOperationRevision {
        self.revision
    }
    #[must_use]
    pub const fn attempt(&self) -> CompactionAttemptNonce {
        self.attempt
    }
    #[must_use]
    pub const fn dispatch_claim(&self) -> Option<CompactionDispatchClaimWitness> {
        self.dispatch_claim
    }
    #[must_use]
    pub const fn request(&self) -> Option<CompactionRequestObservation> {
        self.request
    }
    #[must_use]
    pub const fn provider_frontier(&self) -> Option<CompactionProviderSequence> {
        self.provider_frontier
    }
    #[must_use]
    pub const fn status(&self) -> Option<CompactionStatusObservation> {
        self.status
    }
    #[must_use]
    pub const fn cas_turn(&self) -> Option<&CompactionCasTurnObservation> {
        self.cas_turn.as_ref()
    }
    #[must_use]
    pub const fn marker(&self) -> Option<&CompactionMarkerObservation> {
        self.marker.as_ref()
    }
    #[must_use]
    pub const fn terminal(&self) -> Option<CompactionTerminalObservation> {
        self.terminal
    }
    #[must_use]
    pub const fn state(&self) -> &CompactionOperationState {
        &self.state
    }
}
