use syndic_storage::{
    DraftMutationStagingTerminalEvidenceV1, DraftPieceSettlementClosureV1,
    DraftPieceSettlementOutcomeV1, DraftPieceSettlementProofV1, DraftPieceTransactionOutcomeV1,
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
                Ok(ComposerHostMutationOutcome::Cancelled)
            }
            StagingCommandResult::Source => Err(ComposerHostError::MutationWorkPending),
        }
    }

    pub(super) fn cancel_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        prepared: &PreparedDraftPieceEditV1,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let contribution = self
            .storage
            .cancel_draft_piece_edit(self.storage.revision(store)?, prepared.clone());
        match self.run_build_command(store, prepared, contribution)? {
            BuildCommandResult::Pending(_) => Err(ComposerHostError::MutationWorkPending),
            BuildCommandResult::Terminal(outcome) => {
                self.finish_build_mutation(store, pending, outcome)
            }
        }
    }

    pub(super) fn finish_build_mutation(
        &mut self,
        _store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        outcome: DraftPieceTransactionOutcomeV1,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let result = match outcome {
            DraftPieceTransactionOutcomeV1::Committed(DraftPieceSettlementProofV1::Settlement(
                settlement,
            )) => {
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
                    self.last_request_id = 0;
                    self.lifecycle.adopted(binding, became_dirty);
                }
                ComposerHostMutationOutcome::Committed {
                    binding,
                    positions: pending
                        .intended
                        .ok_or(ComposerHostError::MutationMalformed)?,
                }
            }
            DraftPieceTransactionOutcomeV1::Rejected(_) => ComposerHostMutationOutcome::Rejected,
            DraftPieceTransactionOutcomeV1::Conflict(_) => ComposerHostMutationOutcome::Conflict,
            DraftPieceTransactionOutcomeV1::Cancelled(_) => ComposerHostMutationOutcome::Cancelled,
            DraftPieceTransactionOutcomeV1::Error(_) => ComposerHostMutationOutcome::Error,
            DraftPieceTransactionOutcomeV1::Committed(_) => {
                return Err(ComposerHostError::MutationMalformed);
            }
        };
        Ok(result)
    }
}

pub(super) fn command_selection(
    store: &HomeStore,
    outcome: CommandOutcome,
) -> Result<StagingCommandResult, ComposerHostError> {
    match outcome {
        CommandOutcome::Committed { .. } => Ok(StagingCommandResult::Target),
        CommandOutcome::NotCommitted { .. } => Ok(StagingCommandResult::Source),
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            let handle = reconciliation.install_and_handle();
            Ok(match store.reconcile(&handle)? {
                beryl_home_store::ReconciliationResolution::ExactOld => {
                    StagingCommandResult::Source
                }
                beryl_home_store::ReconciliationResolution::ExactNew { .. } => {
                    StagingCommandResult::Target
                }
                beryl_home_store::ReconciliationResolution::ExactSuccessor { .. }
                | beryl_home_store::ReconciliationResolution::Collision => {
                    StagingCommandResult::Terminal
                }
            })
        }
    }
}

pub(super) fn settlement_outcome(
    settlement: syndic_storage::DraftPieceSettlementV1,
) -> DraftPieceTransactionOutcomeV1 {
    let outcome = settlement.outcome().clone();
    let proof = DraftPieceSettlementProofV1::Settlement(settlement);
    match outcome {
        DraftPieceSettlementOutcomeV1::Committed { .. } => {
            DraftPieceTransactionOutcomeV1::Committed(proof)
        }
        DraftPieceSettlementOutcomeV1::Rejected(_) => {
            DraftPieceTransactionOutcomeV1::Rejected(proof)
        }
        DraftPieceSettlementOutcomeV1::Conflict { .. } => {
            DraftPieceTransactionOutcomeV1::Conflict(proof)
        }
        DraftPieceSettlementOutcomeV1::Cancelled => {
            DraftPieceTransactionOutcomeV1::Cancelled(proof)
        }
        DraftPieceSettlementOutcomeV1::Error(_) => DraftPieceTransactionOutcomeV1::Error(proof),
    }
}
