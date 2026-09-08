use beryl_home_store::{CommandOutcome, ReconciliationResolution};

use super::*;

impl PreparedStagedDraftPieceCommandV1 {
    pub fn submit(self, store: &HomeStore) -> StagedDraftPieceOutcomeFlightV1 {
        let mut flight = StagedDraftPieceOutcomeFlightV1 {
            command: Box::new(self),
            capture: None,
            phase: FlightPhase::NotCommitted,
            classification: StagedDraftPieceDurableClassificationV1::Unresolved,
            receipt: None,
            original_failure: None,
            later_failure: None,
            cleanup_failure: None,
            cleanup_receipt: None,
            cleanup_local_finalization: StagedDraftPieceLocalFinalizationV1::NotRequired,
            failure: None,
            local_finalization: StagedDraftPieceLocalFinalizationV1::NotRequired,
            verification: StagedDraftPieceVerificationWorkV1::default(),
            inert_cleanup: None,
            result: None,
        };
        if !flight.generation_is_current(store) {
            return flight.close(StagedDraftPieceOutcomeErrorV1::Unavailable);
        }
        let capture = Arc::new(Mutex::new(None));
        let storage = &flight.command.storage;
        let mutation = capture::CommandMutation {
            command: flight.command.command.clone(),
            source: flight.command.source.clone(),
            generation: storage.home_generation,
            writer_progress_allowed: writer_progress_is_current(
                storage,
                flight.command.source.admission(),
            ),
            capture: Arc::clone(&capture),
        };
        let outcome = if let CommandKind::Advance(advance) = &flight.command.command {
            let home_revision = match store.home_revision() {
                Ok(revision) => revision,
                Err(source) => {
                    flight.classification = StagedDraftPieceDurableClassificationV1::NotCommitted;
                    flight.original_failure =
                        Some(beryl_home_store::CommandError::RevisionRead { source });
                    return flight;
                }
            };
            let mut command = beryl_home_store::HomeCommand::new(home_revision);
            command
                .add(
                    storage
                        .handle
                        .contribution(advance.expected_revision, mutation),
                )
                .expect("one Syndic participant fits an empty command");
            store.execute(command)
        } else {
            store.execute_current(storage.handle.current_command(mutation))
        };
        flight.capture = capture.lock().ok().and_then(|mut capture| capture.take());
        match outcome {
            CommandOutcome::NotCommitted { evidence } => {
                flight.classification = StagedDraftPieceDurableClassificationV1::NotCommitted;
                flight.original_failure = Some(evidence);
                flight
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                flight.original_failure = Some(failure);
                flight.phase = FlightPhase::Reconciling {
                    handle: reconciliation.install_and_handle(),
                    retry_failed: false,
                };
                flight
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                flight.classification = StagedDraftPieceDurableClassificationV1::Committed;
                flight.receipt = Some(receipt);
                flight.later_failure = later_failure;
                flight.phase = FlightPhase::Finalizing(SelectedSide::Target);
                flight.capture_result();
                if let Some(local_finalization) = local_finalization {
                    flight.local_finalization = StagedDraftPieceLocalFinalizationV1::Consumed;
                    let admission = flight.capture.as_ref().and_then(|capture| {
                        (!capture.replayed)
                            .then(|| capture.source.admission())
                            .flatten()
                    });
                    let result = store.with_committed_local_finalization(
                        local_finalization,
                        flight
                            .receipt
                            .as_ref()
                            .expect("committed outcome retains receipt"),
                        &flight.command.storage.handle,
                        |attachment| match admission {
                            Some(admission) => attachment
                                .finalize_writer_committed_unknown(admission.binding().owner()),
                            None => Ok(()),
                        },
                    );
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(())) => {
                            flight.local_finalization = StagedDraftPieceLocalFinalizationV1::Failed;
                            return flight.close(StagedDraftPieceOutcomeErrorV1::LocalCustody);
                        }
                        Err(error) => {
                            flight.local_finalization = StagedDraftPieceLocalFinalizationV1::Failed;
                            return flight
                                .close(StagedDraftPieceOutcomeErrorV1::LocalFinalization(error));
                        }
                    }
                }
                flight.finish_selected(store, SelectedSide::Target)
            }
        }
    }
}

impl StagedDraftPieceOutcomeFlightV1 {
    pub fn resume(mut self, store: &HomeStore) -> Self {
        if matches!(
            self.phase,
            FlightPhase::Complete | FlightPhase::NotCommitted | FlightPhase::Unavailable(_)
        ) {
            return self;
        }
        if !matches!(
            self.phase,
            FlightPhase::Reconciling { .. } | FlightPhase::CleanupReconciling { .. }
        ) && !self.generation_is_current(store)
        {
            return self.close(StagedDraftPieceOutcomeErrorV1::Unavailable);
        }
        let phase = std::mem::replace(&mut self.phase, FlightPhase::Complete);
        match phase {
            FlightPhase::Reconciling {
                handle,
                retry_failed,
            } => self.reconcile_command(store, handle, retry_failed),
            FlightPhase::Verifying(side) => self.verify_selected(store, side),
            FlightPhase::Finalizing(side) => self.finish_selected(store, side),
            FlightPhase::CleanupPending => self.cleanup_command(store),
            FlightPhase::CleanupReconciling {
                handle,
                retry_failed,
            } => self.reconcile_cleanup(store, handle, retry_failed),
            FlightPhase::CleanupVerifying => self.verify_cleanup(store),
            FlightPhase::CleanupFinalizing => self.finish_cleanup(store),
            phase => {
                self.phase = phase;
                self
            }
        }
    }

    fn reconcile_command(
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
                self.classification = StagedDraftPieceDurableClassificationV1::NotCommitted;
                self.verify_selected(store, SelectedSide::Source)
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                self.classification = StagedDraftPieceDurableClassificationV1::Committed;
                self.receipt = Some(receipt);
                self.capture_result();
                self.verify_selected(store, SelectedSide::Target)
            }
            Ok(ReconciliationResolution::ExactSuccessor { receipt }) => {
                self.receipt = Some(receipt);
                self.phase = FlightPhase::Reconciling {
                    handle,
                    retry_failed,
                };
                self.close(StagedDraftPieceOutcomeErrorV1::ExactSuccessor)
            }
            Ok(ReconciliationResolution::Collision) => {
                self.phase = FlightPhase::Reconciling {
                    handle,
                    retry_failed,
                };
                self.close(StagedDraftPieceOutcomeErrorV1::Collision)
            }
            Err(error) => {
                self.failure = Some(StagedDraftPieceOutcomeErrorV1::Reconciliation(error));
                self.phase = FlightPhase::Reconciling {
                    handle,
                    retry_failed: true,
                };
                self
            }
        }
    }

    fn verify_selected(mut self, store: &HomeStore, side: SelectedSide) -> Self {
        self.phase = FlightPhase::Verifying(side);
        let Some(capture) = &self.capture else {
            return self.close(StagedDraftPieceOutcomeErrorV1::MissingCapture);
        };
        let mut reader = verification::OutcomeReader::new(&self.command.storage, store);
        let result = verification::verify(&mut reader, capture, side);
        self.verification = reader.work();
        match result {
            Ok(()) => {
                self.failure = None;
                self.finish_selected(store, side)
            }
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

    fn finish_selected(mut self, store: &HomeStore, side: SelectedSide) -> Self {
        self.phase = FlightPhase::Finalizing(side);
        let Some(capture) = &self.capture else {
            return self.close(StagedDraftPieceOutcomeErrorV1::MissingCapture);
        };
        if capture.replayed {
            self.failure = None;
            self.phase = match side {
                SelectedSide::Source => FlightPhase::NotCommitted,
                SelectedSide::Target => FlightPhase::Complete,
            };
            return self;
        }
        let selected = match side {
            SelectedSide::Source => &capture.source,
            SelectedSide::Target => &capture.target,
        };
        let admission = selected.admission();
        let owner = admission.map(|admission| admission.binding().owner());
        let Some(settlement) = selected.settlement.as_ref() else {
            if let Some(owner) = owner {
                let result = store.with_domain_attachment(
                    &self.command.storage.handle.attachment_capability(),
                    |attachment| attachment.resolve_writer_progress(owner),
                );
                if !matches!(result, Ok(Ok(()))) {
                    self.failure = Some(StagedDraftPieceOutcomeErrorV1::LocalCustody);
                    return self;
                }
            }
            self.failure = None;
            self.phase = match side {
                SelectedSide::Source => FlightPhase::NotCommitted,
                SelectedSide::Target => FlightPhase::Complete,
            };
            return self;
        };
        if matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Committed { .. }
        ) {
            self.failure = None;
            self.phase = if admission.is_some() {
                FlightPhase::CleanupPending
            } else {
                FlightPhase::Complete
            };
            return self;
        }
        if let Some(admission) = admission {
            if !cleanup::captured_terminal_admission_is_exact(selected, admission) {
                return self.close(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "terminal writer evidence",
                ));
            }
            let owner = admission.binding().owner();
            let result = store.with_domain_attachment(
                &self.command.storage.handle.attachment_capability(),
                |attachment| attachment.resolve_writer_terminal(owner),
            );
            if !matches!(result, Ok(Ok(()))) {
                self.failure = Some(StagedDraftPieceOutcomeErrorV1::LocalCustody);
                return self;
            }
            self.inert_cleanup = Some(StagedDraftPieceInertCleanupV1 {
                admission,
                terminal_digest: settlement.terminal_receipt().digest(),
            });
        }
        self.failure = None;
        self.phase = FlightPhase::Complete;
        self
    }

    fn capture_result(&mut self) {
        self.result = self.capture.as_ref().and_then(|capture| {
            if let Some(settlement) = &capture.target.settlement {
                Some(Box::new(terminal_result(settlement.clone())))
            } else {
                let build = capture.target.build.clone()?;
                let status = match build.lifecycle() {
                    DraftPieceBuildLifecycleV1::Open => DraftPieceOperationStatusV1::Open(build),
                    DraftPieceBuildLifecycleV1::Complete => {
                        DraftPieceOperationStatusV1::Complete(build)
                    }
                    _ => return None,
                };
                Some(Box::new(DraftPieceReconciledCommandV1::Pending(status)))
            }
        });
    }

    pub(super) fn generation_is_current(&self, store: &HomeStore) -> bool {
        store.health().generation() == Some(self.command.storage.home_generation)
    }

    pub(super) fn close(mut self, error: StagedDraftPieceOutcomeErrorV1) -> Self {
        self.failure = Some(error);
        let phase = std::mem::replace(&mut self.phase, FlightPhase::Complete);
        self.phase = FlightPhase::Unavailable(Box::new(phase));
        self
    }
}

fn terminal_result(settlement: DraftPieceSettlementV1) -> DraftPieceReconciledCommandV1 {
    let outcome = settlement.outcome().clone();
    let proof = DraftPieceSettlementProofV1::Settlement(settlement);
    let result = match outcome {
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
    };
    DraftPieceReconciledCommandV1::Terminal(result)
}
