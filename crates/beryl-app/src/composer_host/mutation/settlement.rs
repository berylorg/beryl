use super::*;

impl SyndicComposerHost {
    pub(super) fn finish_mutation(
        &mut self,
        store: &HomeStore,
        outcome: DraftPieceTransactionOutcomeV1,
        positions: MutationPositions,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let pending = self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?;
        let pending = match pending {
            ComposerHostPendingMutation::Staged(pending)
            | ComposerHostPendingMutation::Admitted(pending) => pending,
            ComposerHostPendingMutation::Unavailable(intent) => {
                self.pending_mutation = Some(ComposerHostPendingMutation::Unavailable(intent));
                return Err(ComposerHostError::MutationUnavailable);
            }
        };
        let result = match outcome {
            DraftPieceTransactionOutcomeV1::Committed(DraftPieceSettlementProofV1::Settlement(
                settlement,
            )) => {
                let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure()
                else {
                    self.pending_mutation =
                        Some(ComposerHostPendingMutation::Unavailable(pending.intent));
                    return Err(ComposerHostError::MutationMalformed);
                };
                let candidate =
                    DraftEditorCandidateActivationBindingV1::from_head(adoption.adopted_session());
                if let Err(error) = validate_committed_successor(
                    &self.storage,
                    store,
                    candidate.root(),
                    positions,
                    &pending.successors,
                ) {
                    self.pending_mutation =
                        Some(ComposerHostPendingMutation::Unavailable(pending.intent));
                    return Err(error);
                }
                let binding = ComposerHostBinding::new(
                    pending.binding.home_id(),
                    pending.binding.home_generation(),
                    pending.binding.host_generation(),
                    candidate,
                    pending.binding.presentation_generation(),
                );
                let Some(active) = self.active.as_mut() else {
                    self.pending_mutation =
                        Some(ComposerHostPendingMutation::Unavailable(pending.intent));
                    return Err(ComposerHostError::OldBinding);
                };
                active.binding = binding;
                active.storage_candidate = candidate;
                self.pending.clear();
                self.last_request_id = 0;
                ComposerHostMutationOutcome::Committed { binding, positions }
            }
            DraftPieceTransactionOutcomeV1::Rejected(proof) => {
                self.finish_noncommit(pending.binding.candidate(), proof)?;
                ComposerHostMutationOutcome::Rejected
            }
            DraftPieceTransactionOutcomeV1::Conflict(proof) => {
                self.finish_noncommit(pending.binding.candidate(), proof)?;
                self.pending_mutation =
                    Some(ComposerHostPendingMutation::Unavailable(pending.intent));
                ComposerHostMutationOutcome::Conflict
            }
            DraftPieceTransactionOutcomeV1::Cancelled(proof) => {
                self.finish_noncommit(pending.binding.candidate(), proof)?;
                ComposerHostMutationOutcome::Cancelled
            }
            DraftPieceTransactionOutcomeV1::Error(proof) => {
                self.finish_noncommit(pending.binding.candidate(), proof)?;
                ComposerHostMutationOutcome::Error
            }
            DraftPieceTransactionOutcomeV1::Committed(_) => {
                self.pending_mutation =
                    Some(ComposerHostPendingMutation::Unavailable(pending.intent));
                return Err(ComposerHostError::MutationMalformed);
            }
        };
        Ok(result)
    }

    fn finish_noncommit(
        &mut self,
        logical: DraftEditorCandidateActivationBindingV1,
        proof: DraftPieceSettlementProofV1,
    ) -> Result<(), ComposerHostError> {
        let DraftPieceSettlementProofV1::Settlement(settlement) = proof else {
            return Ok(());
        };
        let DraftPieceSettlementClosureV1::Noncommit(closure) = settlement.closure() else {
            return Err(ComposerHostError::MutationMalformed);
        };
        let refreshed =
            DraftEditorCandidateActivationBindingV1::from_head(closure.observed_session());
        if refreshed.candidate_generation() == logical.candidate_generation()
            && refreshed.root() == logical.root()
            && refreshed.logical_extent() == logical.logical_extent()
        {
            self.active
                .as_mut()
                .ok_or(ComposerHostError::OldBinding)?
                .storage_candidate = refreshed;
        }
        Ok(())
    }

    pub(super) fn synchronize_mutation_candidate(
        &mut self,
        store: &HomeStore,
    ) -> Result<bool, ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        let logical = active.binding.candidate();
        let session = match self.storage.draft_editor_candidate_session(
            store,
            logical.draft_id(),
            logical.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
            _ => {
                candidate_head(&self.storage, store, active.storage_candidate)?;
                unreachable!()
            }
        };
        let refreshed = DraftEditorCandidateActivationBindingV1::from_head(&session);
        if refreshed.candidate_generation() != logical.candidate_generation()
            || refreshed.root() != logical.root()
            || refreshed.logical_extent() != logical.logical_extent()
        {
            return Ok(false);
        }
        self.active
            .as_mut()
            .ok_or(ComposerHostError::OldBinding)?
            .storage_candidate = refreshed;
        Ok(true)
    }
}
