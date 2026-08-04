use beryl_model::{CasTurnId, InputGateRevision};

use super::*;

impl CompactionOperationRecord {
    pub(crate) fn claim_dispatch(
        &self,
        attempt: CompactionAttemptNonce,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.state != CompactionOperationState::Admitted
            || self.dispatch_claim.is_some()
            || self.attempt != attempt
        {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            Some(CompactionDispatchClaimWitness::new(self.revision, attempt)),
            self.request,
            self.provider_frontier,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            CompactionOperationState::DispatchClaimed,
        )
    }

    pub(crate) fn observe_request(
        &self,
        disposition: CompactionRequestDisposition,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.dispatch_claim.is_none() || self.request.is_some() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        let state = match (&self.state, disposition) {
            (CompactionOperationState::DispatchClaimed, CompactionRequestDisposition::Accepted) => {
                CompactionOperationState::Live
            }
            (
                CompactionOperationState::DispatchClaimed,
                CompactionRequestDisposition::RejectedBeforeCore
                | CompactionRequestDisposition::ProvenLocalNondispatch
                | CompactionRequestDisposition::CompletionUnknown,
            ) => CompactionOperationState::DispatchClaimed,
            (CompactionOperationState::Live, CompactionRequestDisposition::Accepted)
            | (CompactionOperationState::Finalizing, CompactionRequestDisposition::Accepted)
            | (
                CompactionOperationState::Finalizing,
                CompactionRequestDisposition::CompletionUnknown,
            ) => self.state.clone(),
            _ => return Err(crate::SyndicRecordError::InvalidCompactionOperation),
        };
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            self.dispatch_claim,
            Some(CompactionRequestObservation::new(revision, disposition)),
            self.provider_frontier,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            state,
        )
    }
}

impl CompactionOperationRecord {
    pub(crate) fn consume(
        &self,
        successor_gate_revision: InputGateRevision,
        settlement: CompactionSettlement,
        receipt_commitment: CompactionSettlementReceiptCommitment,
    ) -> Result<Self, crate::SyndicRecordError> {
        if matches!(
            self.state,
            CompactionOperationState::Consumed(_) | CompactionOperationState::Stopping(_)
        ) {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        self.consume_successor(successor_gate_revision, settlement, receipt_commitment)
    }

    pub(crate) fn handoff_to_stop(
        &self,
        stop_nonce: StopOperationNonce,
    ) -> Result<Self, crate::SyndicRecordError> {
        if !matches!(
            self.state,
            CompactionOperationState::DispatchClaimed | CompactionOperationState::Live
        ) || self.cas_turn.is_none()
            || self.terminal.is_some()
        {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            self.dispatch_claim,
            self.request,
            self.provider_frontier,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            CompactionOperationState::Stopping(stop_nonce),
        )
    }

    pub(crate) fn reopen_from_stop(
        &self,
        stop_nonce: StopOperationNonce,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.state != CompactionOperationState::Stopping(stop_nonce) || self.terminal.is_some() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            self.dispatch_claim,
            self.request,
            self.provider_frontier,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            if self.request.is_some_and(|request| {
                request.disposition() == CompactionRequestDisposition::Accepted
            }) {
                CompactionOperationState::Live
            } else {
                CompactionOperationState::DispatchClaimed
            },
        )
    }

    pub(crate) fn abandon_from_stop(
        &self,
        stop_nonce: StopOperationNonce,
        successor_gate_revision: InputGateRevision,
        settlement: CompactionSettlement,
        receipt_commitment: CompactionSettlementReceiptCommitment,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.state != CompactionOperationState::Stopping(stop_nonce) || self.terminal.is_some() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        self.consume_successor(successor_gate_revision, settlement, receipt_commitment)
    }

    fn consume_successor(
        &self,
        successor_gate_revision: InputGateRevision,
        settlement: CompactionSettlement,
        receipt_commitment: CompactionSettlementReceiptCommitment,
    ) -> Result<Self, crate::SyndicRecordError> {
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            self.dispatch_claim,
            self.request,
            self.provider_frontier,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            CompactionOperationState::Consumed(CompactionConsumedWitness::new(
                self.revision,
                successor_gate_revision,
                settlement,
                receipt_commitment,
            )),
        )
    }
}

impl CompactionOperationRecord {
    pub(crate) fn consumed_witness_is_exact(&self) -> bool {
        let CompactionOperationState::Consumed(witness) = &self.state else {
            return false;
        };
        if witness.source_revision().checked_next().ok() != Some(self.revision) {
            return false;
        }
        match witness.settlement() {
            CompactionSettlement::CancelledBeforeDispatch => {
                self.dispatch_claim.is_none() && self.request.is_none() && self.terminal.is_none()
            }
            CompactionSettlement::LocalNondispatch => self.request.is_some_and(|request| {
                request.disposition() == CompactionRequestDisposition::ProvenLocalNondispatch
            }),
            CompactionSettlement::Abandoned(reason) => match reason {
                crate::CompactionAbandonmentReason::ProviderRejectedBeforeCore => {
                    self.request.is_some_and(|request| {
                        request.disposition() == CompactionRequestDisposition::RejectedBeforeCore
                    })
                }
                crate::CompactionAbandonmentReason::CompletionUnknown => {
                    self.request.is_some_and(|request| {
                        request.disposition() == CompactionRequestDisposition::CompletionUnknown
                    })
                }
                crate::CompactionAbandonmentReason::TargetAuthorityLost
                | crate::CompactionAbandonmentReason::StartupProcessGenerationLost
                | crate::CompactionAbandonmentReason::ProviderProtocolConflict => true,
            },
            CompactionSettlement::ManualSuccess
            | CompactionSettlement::LifecycleUserWorkWon
            | CompactionSettlement::LifecycleContinuation { .. } => {
                self.terminal.is_some_and(|terminal| {
                    terminal.status().outcome() == crate::TurnTerminalOutcome::Complete
                }) && self.marker.as_ref().is_some_and(|marker| {
                    marker.lifecycle() == CompactionMarkerLifecycle::Completed
                })
            }
            CompactionSettlement::ManualFailure => self.terminal.is_some(),
        }
    }

    pub(crate) fn consumed_receipt_is_exact(
        &self,
        receipt: &crate::CompactionSettlementReceiptRecord,
    ) -> bool {
        let CompactionOperationState::Consumed(witness) = &self.state else {
            return false;
        };
        self.consumed_witness_is_exact()
            && receipt.operation_id() == self.id
            && receipt.source_operation_revision() == witness.source_revision()
            && receipt.successor_operation_revision() == self.revision
            && receipt.successor_gate().revision() == witness.successor_gate_revision()
            && receipt.settlement() == witness.settlement()
            && crate::codec::compaction_settlement_receipt_commitment(receipt).ok()
                == Some(witness.receipt_commitment())
    }

    pub(crate) fn observe_status(
        &self,
        sequence: CompactionProviderSequence,
        status: CompactionThreadStatus,
    ) -> Result<Self, crate::SyndicRecordError> {
        self.observe_provider(
            sequence,
            Some(CompactionStatusObservation::new(sequence, status)),
            self.cas_turn.clone(),
            self.marker.clone(),
            self.terminal,
            self.state.clone(),
        )
    }

    pub(crate) fn observe_cas_turn(
        &self,
        sequence: CompactionProviderSequence,
        cas_turn_id: CasTurnId,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.cas_turn.is_some() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        self.observe_provider(
            sequence,
            self.status,
            Some(CompactionCasTurnObservation::new(sequence, cas_turn_id)),
            self.marker.clone(),
            self.terminal,
            self.state.clone(),
        )
    }

    pub(crate) fn observe_marker(
        &self,
        marker: CompactionMarkerObservation,
    ) -> Result<Self, crate::SyndicRecordError> {
        let valid = match self.marker.as_ref() {
            None => marker.lifecycle() == CompactionMarkerLifecycle::Started,
            Some(previous) => {
                previous.item_id() == marker.item_id()
                    && previous.lifecycle() == CompactionMarkerLifecycle::Started
                    && marker.lifecycle() == CompactionMarkerLifecycle::Completed
            }
        };
        if !valid || self.cas_turn.is_none() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        self.observe_provider(
            marker.sequence(),
            self.status,
            self.cas_turn.clone(),
            Some(marker),
            self.terminal,
            self.state.clone(),
        )
    }

    pub(crate) fn observe_terminal(
        &self,
        sequence: CompactionProviderSequence,
        status: TurnEndStatus,
        turn_state_revision: TurnStateRevision,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.terminal.is_some() || self.cas_turn.is_none() {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        self.observe_provider(
            sequence,
            self.status,
            self.cas_turn.clone(),
            self.marker.clone(),
            Some(CompactionTerminalObservation::new(
                sequence,
                status,
                turn_state_revision,
            )),
            CompactionOperationState::Finalizing,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn observe_provider(
        &self,
        sequence: CompactionProviderSequence,
        status: Option<CompactionStatusObservation>,
        cas_turn: Option<CompactionCasTurnObservation>,
        marker: Option<CompactionMarkerObservation>,
        terminal: Option<CompactionTerminalObservation>,
        state: CompactionOperationState,
    ) -> Result<Self, crate::SyndicRecordError> {
        if self.dispatch_claim.is_none()
            || matches!(
                self.state,
                CompactionOperationState::Admitted | CompactionOperationState::Consumed(_)
            )
            || self
                .provider_frontier
                .map_or(sequence != CompactionProviderSequence::FIRST, |frontier| {
                    frontier.checked_next().ok() != Some(sequence)
                })
        {
            return Err(crate::SyndicRecordError::InvalidCompactionOperation);
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        Self::new(
            self.id,
            self.home_id,
            self.target.clone(),
            revision,
            self.attempt,
            self.dispatch_claim,
            self.request,
            Some(sequence),
            status,
            cas_turn,
            marker,
            terminal,
            state,
        )
    }
}
