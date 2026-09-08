use beryl_home_store::CommandError;
use syndic_storage::{
    DraftPieceOperationStatusV1, DraftPieceReconciledCommandV1, DraftPieceSettlementProofV1,
    DraftPieceTransactionOutcomeV1, PreparedStagedDraftPieceCommandV1,
    StagedDraftPieceCommandCompletionV1, StagedDraftPieceDurableClassificationV1,
    StagedDraftPieceOutcomeErrorV1, StagedDraftPieceOutcomeStateV1,
};

use super::*;

#[derive(Default)]
pub(in crate::composer_host) struct BuildDiagnostics {
    classification: Option<StagedDraftPieceDurableClassificationV1>,
    result: Option<DraftPieceReconciledCommandV1>,
    original_failure: Option<CommandError>,
    later_failure: Option<CommandError>,
    cleanup_failure: Option<CommandError>,
    local_failure: Option<StagedDraftPieceOutcomeErrorV1>,
}

pub struct ComposerHostMutationBuildDiagnostics<'a> {
    pub state: Option<StagedDraftPieceOutcomeStateV1>,
    pub classification: Option<StagedDraftPieceDurableClassificationV1>,
    pub result: Option<&'a DraftPieceReconciledCommandV1>,
    pub original_failure: Option<&'a CommandError>,
    pub later_failure: Option<&'a CommandError>,
    pub cleanup_failure: Option<&'a CommandError>,
    pub local_failure: Option<&'a StagedDraftPieceOutcomeErrorV1>,
}

impl BuildDiagnostics {
    fn view<'a>(
        &'a self,
        flight: Option<&'a syndic_storage::StagedDraftPieceOutcomeFlightV1>,
    ) -> ComposerHostMutationBuildDiagnostics<'a> {
        ComposerHostMutationBuildDiagnostics {
            state: flight.map(|flight| flight.state()),
            classification: flight
                .map(|flight| flight.classification())
                .or(self.classification),
            result: flight
                .and_then(|flight| flight.result())
                .or(self.result.as_ref()),
            original_failure: self
                .original_failure
                .as_ref()
                .or_else(|| flight.and_then(|flight| flight.original_failure())),
            later_failure: self
                .later_failure
                .as_ref()
                .or_else(|| flight.and_then(|flight| flight.later_failure())),
            cleanup_failure: self
                .cleanup_failure
                .as_ref()
                .or_else(|| flight.and_then(|flight| flight.cleanup_failure())),
            local_failure: self
                .local_failure
                .as_ref()
                .or_else(|| flight.and_then(|flight| flight.failure())),
        }
    }

    fn retain_completion(&mut self, completion: &mut StagedDraftPieceCommandCompletionV1) {
        self.classification = Some(StagedDraftPieceDurableClassificationV1::Committed);
        self.result = Some(completion.result.clone());
        if self.original_failure.is_none() {
            self.original_failure = completion.original_failure.take();
        }
        if self.later_failure.is_none() {
            self.later_failure = completion.later_failure.take();
        }
        if self.cleanup_failure.is_none() {
            self.cleanup_failure = completion.cleanup_failure.take();
        }
    }
}

impl ComposerHostMutationCoordinator {
    pub(super) fn unavailable_error(&self) -> ComposerHostError {
        let result = self
            .build_flight
            .as_ref()
            .and_then(|flight| flight.result())
            .or_else(|| {
                self.build_completion
                    .as_ref()
                    .map(|completion| &completion.result)
            });
        if matches!(
            result,
            Some(DraftPieceReconciledCommandV1::Terminal(
                DraftPieceTransactionOutcomeV1::Committed(_)
            ))
        ) {
            ComposerHostError::MutationCommittedUnavailable
        } else if matches!(result, Some(DraftPieceReconciledCommandV1::Pending(_))) {
            ComposerHostError::MutationAdmittedWorkUnavailable
        } else {
            ComposerHostError::MutationUnavailable
        }
    }

    pub(super) fn build_endpoint(
        &self,
    ) -> Result<DraftPieceBuildProgressReceiptReferenceV1, ComposerHostError> {
        match self.phase {
            ComposerHostMutationPhase::Building { endpoint } => Ok(endpoint),
            _ => Err(ComposerHostError::MutationMalformed),
        }
    }
}

impl SyndicComposerHost {
    pub fn mutation_build_diagnostics(&self) -> Option<ComposerHostMutationBuildDiagnostics<'_>> {
        match &self.pending_mutation {
            Some(
                ComposerHostPendingMutation::Active(pending)
                | ComposerHostPendingMutation::Unavailable(pending),
            ) => Some(
                pending
                    .build_diagnostics
                    .view(pending.build_flight.as_ref()),
            ),
            _ => self
                .last_mutation_build_diagnostics
                .as_ref()
                .map(|diagnostics| diagnostics.view(None)),
        }
    }

    pub(super) fn run_build_command(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        command: PreparedStagedDraftPieceCommandV1,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        if pending.build_flight.is_some() {
            return Err(ComposerHostError::MutationWorkPending);
        }
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.mutation_before_execute_fault.take() {
            fault(store, self.storage.clone());
        }
        pending.build_flight = Some(command.submit(store));
        self.consume_build_outcome(pending)
    }

    pub(super) fn resume_build_command(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let flight = pending
            .build_flight
            .take()
            .ok_or(ComposerHostError::MutationMalformed)?;
        #[cfg(feature = "test-faults")]
        if flight.state() == StagedDraftPieceOutcomeStateV1::CleanupPending
            && let Some(fault) = self.mutation_before_execute_fault.take()
        {
            fault(store, self.storage.clone());
        }
        pending.build_flight = Some(flight.resume(store));
        self.consume_build_outcome(pending)
    }

    fn consume_build_outcome(
        &mut self,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let flight = pending
            .build_flight
            .take()
            .ok_or(ComposerHostError::MutationMalformed)?;
        let mut completion = match flight.into_completion() {
            Ok(completion) => completion,
            Err(flight) => match flight.into_noncommit() {
                Ok(noncommit) => {
                    pending.build_diagnostics.classification =
                        Some(StagedDraftPieceDurableClassificationV1::NotCommitted);
                    if pending.build_diagnostics.original_failure.is_none() {
                        pending.build_diagnostics.original_failure = Some(noncommit.failure);
                    }
                    if pending.build_diagnostics.local_failure.is_none() {
                        pending.build_diagnostics.local_failure = noncommit.local_failure;
                    }
                    pending.build_noncommit = Some(noncommit.command);
                    return Err(ComposerHostError::MutationWorkPending);
                }
                Err(flight) => {
                    let unavailable = flight.state() == StagedDraftPieceOutcomeStateV1::Unavailable;
                    pending.build_flight = Some(flight);
                    return Err(if unavailable {
                        pending.unavailable_error()
                    } else {
                        ComposerHostError::MutationWorkPending
                    });
                }
            },
        };
        pending.build_diagnostics.retain_completion(&mut completion);
        match completion.result.clone() {
            DraftPieceReconciledCommandV1::Pending(
                DraftPieceOperationStatusV1::Open(build)
                | DraftPieceOperationStatusV1::Complete(build),
            ) => Ok(BuildCommandResult::Pending(build.progress_receipt())),
            DraftPieceReconciledCommandV1::Terminal(transaction) => {
                pending.build_completion = Some(completion);
                let proof = match transaction {
                    DraftPieceTransactionOutcomeV1::Committed(proof)
                    | DraftPieceTransactionOutcomeV1::Rejected(proof)
                    | DraftPieceTransactionOutcomeV1::Conflict(proof)
                    | DraftPieceTransactionOutcomeV1::Cancelled(proof)
                    | DraftPieceTransactionOutcomeV1::Error(proof) => proof,
                };
                let DraftPieceSettlementProofV1::Settlement(settlement) = proof else {
                    return Err(pending.unavailable_error());
                };
                let outcome = self
                    .finish_build_settlement(pending, settlement)
                    .map_err(|_| pending.unavailable_error())?;
                let completion = pending
                    .build_completion
                    .take()
                    .ok_or(ComposerHostError::MutationMalformed)?;
                if let Some(owner) = completion.inert_cleanup {
                    pending.cleanup = Some(cleanup::ComposerHostMutationCleanup::from_inert(
                        owner,
                        outcome.clone(),
                    ));
                }
                Ok(BuildCommandResult::Terminal(outcome))
            }
            _ => Err(ComposerHostError::MutationUnavailable),
        }
    }
}
