use beryl_home_store::{CommandOutcome, ReconciliationResolution};

use super::*;
use crate::draft_piece::admission::closure::terminal_receipt_is_exact;

impl StagedDraftPieceOutcomeFlightV1 {
    pub(super) fn cleanup_command(mut self, store: &HomeStore) -> Self {
        self.phase = FlightPhase::CleanupPending;
        let Some(settlement) = self
            .capture
            .as_ref()
            .and_then(|capture| capture.target.settlement.clone())
        else {
            return self.close(StagedDraftPieceOutcomeErrorV1::MissingCapture);
        };
        let Some(admission) = settlement
            .terminal_source()
            .and_then(DraftPieceBuildRecordV1::writer_admission)
        else {
            return self.close(StagedDraftPieceOutcomeErrorV1::Invariant(
                "committed writer admission",
            ));
        };
        if !admission.is_empty()
            || !matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed { .. }
            )
        {
            return self.close(StagedDraftPieceOutcomeErrorV1::Invariant(
                "committed writer targets remain",
            ));
        }
        let outcome = store.execute_current(
            self.command
                .storage
                .handle
                .current_command(ReleaseSettledWriterMutation { settlement }),
        );
        match outcome {
            CommandOutcome::NotCommitted { evidence } => {
                self.cleanup_failure = Some(evidence);
                self
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                self.cleanup_failure = Some(failure);
                self.phase = FlightPhase::CleanupReconciling {
                    handle: reconciliation.install_and_handle(),
                    retry_failed: false,
                };
                self
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                self.cleanup_receipt = Some(receipt);
                if later_failure.is_some() {
                    self.cleanup_failure = later_failure;
                }
                self.phase = FlightPhase::CleanupFinalizing;
                if let Some(local_finalization) = local_finalization {
                    self.cleanup_local_finalization = StagedDraftPieceLocalFinalizationV1::Consumed;
                    let result = store.with_committed_local_finalization(
                        local_finalization,
                        self.cleanup_receipt
                            .as_ref()
                            .expect("cleanup commit retains its receipt"),
                        &self.command.storage.handle,
                        |attachment| {
                            attachment.resolve_terminal(admission.binding().owner(), true, false)
                        },
                    );
                    match result {
                        Ok(Ok(())) => {
                            self.failure = None;
                            self.phase = FlightPhase::Complete;
                            self
                        }
                        Ok(Err(())) => {
                            self.cleanup_local_finalization =
                                StagedDraftPieceLocalFinalizationV1::Failed;
                            self.close(StagedDraftPieceOutcomeErrorV1::LocalCustody)
                        }
                        Err(error) => {
                            self.cleanup_local_finalization =
                                StagedDraftPieceLocalFinalizationV1::Failed;
                            self.close(StagedDraftPieceOutcomeErrorV1::LocalFinalization(error))
                        }
                    }
                } else {
                    self.finish_cleanup(store)
                }
            }
        }
    }

    pub(super) fn reconcile_cleanup(
        mut self,
        store: &HomeStore,
        handle: ReconciliationHandle,
        retry_failed: bool,
    ) -> Self {
        let resolution = if retry_failed {
            store.retry_reconciliation(&handle)
        } else {
            store.reconcile(&handle)
        };
        match resolution {
            Ok(ReconciliationResolution::ExactOld) => {
                self.failure = None;
                self.phase = FlightPhase::CleanupPending;
                self
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                self.cleanup_receipt = Some(receipt);
                self.phase = FlightPhase::CleanupVerifying;
                self.verify_cleanup(store)
            }
            Ok(ReconciliationResolution::ExactSuccessor { .. }) => {
                self.phase = FlightPhase::CleanupReconciling {
                    handle,
                    retry_failed,
                };
                self.close(StagedDraftPieceOutcomeErrorV1::ExactSuccessor)
            }
            Ok(ReconciliationResolution::Collision) => {
                self.phase = FlightPhase::CleanupReconciling {
                    handle,
                    retry_failed,
                };
                self.close(StagedDraftPieceOutcomeErrorV1::Collision)
            }
            Err(error) => {
                self.failure = Some(StagedDraftPieceOutcomeErrorV1::Reconciliation(error));
                self.phase = FlightPhase::CleanupReconciling {
                    handle,
                    retry_failed: true,
                };
                self
            }
        }
    }

    pub(super) fn verify_cleanup(mut self, store: &HomeStore) -> Self {
        self.phase = FlightPhase::CleanupVerifying;
        let Some(capture) = self.capture.as_ref() else {
            return self.close(StagedDraftPieceOutcomeErrorV1::MissingCapture);
        };
        let mut reader = verification::OutcomeReader::new(&self.command.storage, store);
        let result = verification::verify_cleanup(&mut reader, &capture.target);
        self.verification = reader.work();
        match result {
            Ok(()) => self.finish_cleanup(store),
            Err(
                error @ (StagedDraftPieceOutcomeErrorV1::Read(_)
                | StagedDraftPieceOutcomeErrorV1::ConcurrentChange),
            ) => {
                self.failure = Some(error);
                self
            }
            Err(error) => self.close(error),
        }
    }

    pub(super) fn finish_cleanup(mut self, store: &HomeStore) -> Self {
        self.phase = FlightPhase::CleanupFinalizing;
        let Some(admission) = self
            .capture
            .as_ref()
            .and_then(|capture| capture.target.admission())
        else {
            return self.close(StagedDraftPieceOutcomeErrorV1::MissingCapture);
        };
        let result = store.with_domain_attachment(
            &self.command.storage.handle.attachment_capability(),
            |attachment| attachment.resolve_terminal(admission.binding().owner(), true, false),
        );
        if matches!(result, Ok(Ok(()))) {
            self.failure = None;
            self.phase = FlightPhase::Complete;
        } else {
            self.failure = Some(StagedDraftPieceOutcomeErrorV1::LocalCustody);
        }
        self
    }
}

pub(super) fn captured_terminal_admission_is_exact(
    selected: &CapturedState,
    admission: DraftMarkerWriterAdmissionV1,
) -> bool {
    let Some(settlement) = selected.settlement.as_ref() else {
        return false;
    };
    let Some((head, receipt)) = &selected.terminal_admission else {
        return false;
    };
    let owner = admission.binding().owner();
    let key = DraftMarkerAdmissionReceiptKeyV1::new(
        owner,
        staging_terminal_command(owner, settlement.terminal_receipt().digest()),
    );
    head.owner() == owner
        && head.lifecycle() == DraftMarkerAdmissionLifecycleV1::TerminalCleanup
        && head.home_generation() == admission.binding().home_generation()
        && head.source_root().count() == 0
        && head.target_root().count() == 0
        && head.remaining_builder_count() == 0
        && head.occurrence_commitment() == admission.binding().occurrence_commitment()
        && terminal_receipt_is_exact(head, key, receipt)
}
