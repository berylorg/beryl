use syndic_storage::{
    DraftMutationStagingTerminalEvidenceV1, DraftPieceSettlementClosureV1,
    DraftPieceSettlementOutcomeV1, DraftPieceSettlementV1,
};

use super::*;

impl SyndicComposerHost {
    pub(super) fn cancel_staging_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let evidence = DraftMutationStagingTerminalEvidenceV1::Cancelled {
            request_id: pending.identity.operation_id(),
            source_lifecycle: pending.head.lifecycle(),
            writer_admitted: true,
            candidate_generation: pending.session.newest_candidate_generation(),
            root: pending.session.newest_root(),
            history: pending.session.newest_history(),
            session_revision: pending.session.session_generation(),
        };
        let prepared = self.storage.prepare_draft_mutation_staging_terminal(
            &pending.head,
            &pending.session,
            evidence,
        )?;
        match self.run_staging_command(store, &prepared, None)? {
            StagingCommandResult::Target | StagingCommandResult::Terminal => {
                pending.session = prepared
                    .target_session()
                    .cloned()
                    .ok_or(ComposerHostError::MutationMalformed)?;
                Ok(ComposerHostMutationOutcome::Cancelled)
            }
            StagingCommandResult::Source => Err(ComposerHostError::MutationWorkPending),
        }
    }

    pub(super) fn cancel_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let command = self.storage.prepare_staged_draft_piece_terminal(
            store,
            pending.identity,
            pending.build_endpoint()?,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Cancel,
        )?;
        match self.run_build_command(store, pending, command)? {
            BuildCommandResult::Pending(_) => Err(ComposerHostError::MutationWorkPending),
            BuildCommandResult::Terminal(outcome) => Ok(outcome),
        }
    }

    pub(super) fn finish_build_settlement(
        &mut self,
        pending: &mut ComposerHostMutationCoordinator,
        settlement: DraftPieceSettlementV1,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let result = match settlement.outcome() {
            DraftPieceSettlementOutcomeV1::Committed { .. } => {
                let positions = pending
                    .intended
                    .ok_or(ComposerHostError::MutationMalformed)?;
                let became_dirty = !self.is_dirty();
                let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure()
                else {
                    return Err(ComposerHostError::MutationMalformed);
                };
                let candidate =
                    DraftEditorCandidateActivationBindingV1::from_head(adoption.adopted_session());
                let binding = ComposerHostBinding::new(
                    pending.binding.home_id(),
                    pending.binding.home_generation(),
                    pending.binding.host_generation(),
                    candidate,
                    pending.binding.presentation_generation(),
                );
                if !pending.detached {
                    let active = self.active.as_mut().ok_or(ComposerHostError::OldBinding)?;
                    if active.binding != pending.binding {
                        return Err(ComposerHostError::OldBinding);
                    }
                    active.binding = binding;
                    active.storage_candidate = candidate;
                    self.pending.clear();
                    self.lifecycle.adopted(binding, became_dirty);
                }
                ComposerHostMutationOutcome::Committed { binding, positions }
            }
            DraftPieceSettlementOutcomeV1::Rejected(_) => ComposerHostMutationOutcome::Rejected,
            DraftPieceSettlementOutcomeV1::Conflict { .. } => ComposerHostMutationOutcome::Conflict,
            DraftPieceSettlementOutcomeV1::Cancelled if pending.build_noncommit.is_some() => {
                ComposerHostMutationOutcome::Error
            }
            DraftPieceSettlementOutcomeV1::Cancelled => ComposerHostMutationOutcome::Cancelled,
            DraftPieceSettlementOutcomeV1::Error(_) => ComposerHostMutationOutcome::Error,
        };
        if !matches!(result, ComposerHostMutationOutcome::Committed { .. }) {
            let DraftPieceSettlementClosureV1::Noncommit(closure) = settlement.closure() else {
                return Err(ComposerHostError::MutationMalformed);
            };
            pending.session = closure.observed_session().clone();
        }
        Ok(result)
    }

    pub(super) fn adopt_terminal_noncommit_session(
        &mut self,
        pending: &ComposerHostMutationCoordinator,
    ) -> Result<(), ComposerHostError> {
        if pending.detached {
            return Ok(());
        }
        let candidate = DraftEditorCandidateActivationBindingV1::from_head(&pending.session);
        let predecessor = pending.binding.candidate();
        if pending.session.active_operation().is_some()
            || candidate.draft_id() != predecessor.draft_id()
            || candidate.session_id() != predecessor.session_id()
            || candidate.session_generation() < predecessor.session_generation()
            || candidate.candidate_generation() != predecessor.candidate_generation()
            || candidate.root() != predecessor.root()
            || candidate.history() != predecessor.history()
            || candidate.logical_extent() != predecessor.logical_extent()
        {
            return Err(ComposerHostError::OldBinding);
        }
        let active = self.active.as_mut().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != pending.binding {
            return Err(ComposerHostError::OldBinding);
        }
        active.binding = ComposerHostBinding::new(
            pending.binding.home_id(),
            pending.binding.home_generation(),
            pending.binding.host_generation(),
            candidate,
            pending.binding.presentation_generation(),
        );
        active.storage_candidate = candidate;
        Ok(())
    }
}
