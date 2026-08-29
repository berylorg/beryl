mod acceptance;
mod diagnostics;
mod disposal;
mod model;

use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeStore, ReconciliationHandle,
    ReconciliationResolution,
};
use beryl_state::{AssetOwner, AssetState};
use syndic_storage::{
    DraftComposerBuildKeyV1, DraftComposerFormatV1, DraftComposerMaterializationStatusV1,
    DraftPieceOperationIdV1, FirstAcceptance, SyndicPointReadLimit, SyndicTimestamp,
};

use super::*;
pub(super) use model::ComposerHostSubmissionCoordinator;
#[cfg(feature = "test-faults")]
pub use model::ComposerHostSubmissionFaultPoint;
use model::{CapturedSubmission, PendingSubmission, PendingSubmissionStage};
pub use model::{
    ComposerHostSubmissionAdvance, ComposerHostSubmissionDiagnostics, ComposerHostSubmissionError,
    ComposerHostSubmissionRequest, ComposerHostSubmissionStage, ComposerHostSubmissionTicket,
};

impl SyndicComposerHost {
    pub fn begin_submission(
        &mut self,
        request: ComposerHostSubmissionRequest,
    ) -> Result<ComposerHostSubmissionTicket, ComposerHostSubmissionError> {
        if self.lifecycle.is_service_disposed()
            || self.submission.pending.is_some()
            || self.live_operation_pending()
        {
            return Err(ComposerHostError::LifecycleBlocked.into());
        }
        let binding = self.binding().ok_or(ComposerHostError::OldBinding)?;
        let flush = self.begin_flush(ComposerHostFlushPurpose::Submission)?;
        let stage = match flush {
            ComposerHostFlushAdmission::Started { ticket, .. }
            | ComposerHostFlushAdmission::Joined { ticket, .. } => {
                PendingSubmissionStage::Flushing(ticket)
            }
            ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::Submission) => {
                PendingSubmissionStage::Capturing
            }
            ComposerHostFlushAdmission::Satisfied(_) => {
                return Err(ComposerHostError::LifecycleBlocked.into());
            }
        };
        let generation = self
            .submission
            .generation
            .checked_add(1)
            .ok_or(ComposerHostError::GenerationExhausted)?;
        let ticket = ComposerHostSubmissionTicket {
            binding,
            generation,
        };
        self.submission.generation = generation;
        self.submission.pending = Some(Box::new(PendingSubmission {
            ticket,
            request,
            cancellation: None,
            stage,
        }));
        Ok(ticket)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn advance_submission(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostSubmissionTicket,
        assets: AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        publication_operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        if self.lifecycle.is_service_disposed()
            || self
                .submission
                .pending
                .as_ref()
                .is_none_or(|pending| pending.ticket != ticket)
        {
            return Ok(ComposerHostSubmissionAdvance::Stale);
        }
        let cancellation = {
            let pending = self.submission.pending.as_mut().unwrap();
            pending
                .cancellation
                .get_or_insert_with(|| cancellation.clone())
                .clone()
        };
        let stage = {
            let pending = self.submission.pending.as_mut().unwrap();
            std::mem::replace(&mut pending.stage, PendingSubmissionStage::Transitioning)
        };
        let original = stage.clone();
        let outcome = match stage {
            PendingSubmissionStage::Flushing(flush) => self.advance_submission_flush(
                store,
                flush,
                assets,
                marker_seals,
                publication_operation_id,
                marker_authority,
                published_at,
                &cancellation,
            ),
            PendingSubmissionStage::Capturing => {
                if cancellation.is_cancelled() {
                    self.cancel_submission()
                } else {
                    self.capture_submission(store, assets)
                }
            }
            PendingSubmissionStage::Materializing {
                captured,
                reconciliation,
            } => {
                self.advance_submission_materializer(store, captured, reconciliation, &cancellation)
            }
            PendingSubmissionStage::Accepting {
                acceptance,
                reconciliation,
            } => self.advance_submission_acceptance(
                store,
                assets,
                acceptance,
                reconciliation,
                &cancellation,
            ),
            PendingSubmissionStage::Transitioning => {
                unreachable!("submission transition is synchronous")
            }
        };
        if outcome.is_err() {
            if let Some(pending) = self.submission.pending.as_mut().filter(|pending| {
                pending.ticket == ticket
                    && matches!(pending.stage, PendingSubmissionStage::Transitioning)
            }) {
                pending.stage = original;
            }
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_submission_flush(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
        assets: AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        publication_operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        #[cfg(feature = "test-faults")]
        self.inject_submission_transition_fault(ComposerHostSubmissionFaultPoint::Flush)?;
        let state = self.flush_state(flush)?;
        let outcome = if state == ComposerHostFlushState::CaptureRequired {
            match self.capture_flush_publication(
                store,
                flush,
                assets,
                marker_seals,
                publication_operation_id,
                marker_authority,
                published_at,
                cancellation,
            )? {
                ComposerHostFlushCapture::Captured(_) | ComposerHostFlushCapture::State(_) => {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Flushing(flush);
                    ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Flushing)
                }
                ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission) => {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Capturing;
                    ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Capturing)
                }
                ComposerHostFlushCapture::Unsatisfied(_) => {
                    self.submission.pending = None;
                    ComposerHostSubmissionAdvance::NotCommitted
                }
                ComposerHostFlushCapture::Stale | ComposerHostFlushCapture::Satisfied(_) => {
                    self.submission.pending = None;
                    ComposerHostSubmissionAdvance::Stale
                }
            }
        } else {
            match self.advance_flush(store, flush)? {
                ComposerHostFlushAdvance::Progress(_)
                | ComposerHostFlushAdvance::ReconciliationPending => {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Flushing(flush);
                    ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Flushing)
                }
                ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission) => {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Capturing;
                    ComposerHostSubmissionAdvance::Progress(ComposerHostSubmissionStage::Capturing)
                }
                ComposerHostFlushAdvance::Unsatisfied(_) => {
                    self.submission.pending = None;
                    ComposerHostSubmissionAdvance::NotCommitted
                }
                ComposerHostFlushAdvance::Stale | ComposerHostFlushAdvance::Satisfied(_) => {
                    self.submission.pending = None;
                    ComposerHostSubmissionAdvance::Stale
                }
            }
        };
        Ok(outcome)
    }

    fn capture_submission(
        &mut self,
        store: &HomeStore,
        assets: AssetState,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        let (request, candidate, thread_id, published_pair) = {
            let pending = self.submission.pending.as_ref().unwrap();
            let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
            (
                pending.request,
                active.storage_candidate,
                active.thread_id,
                active.published_pair,
            )
        };
        if candidate.root() != published_pair.root()
            || candidate.history() != published_pair.history()
            || self.is_dirty()
        {
            return Err(ComposerHostError::PublicationPending.into());
        }
        let current = self
            .storage
            .current_draft(store, thread_id, submission_point_limit())?
            .ok_or(ComposerHostError::MissingCurrentDraft)?;
        let image_label_authority = self
            .storage
            .image_label_authority_head(store, thread_id, submission_point_limit())?
            .ok_or(ComposerHostError::MissingImageLabelAuthority)?;
        if current.draft().id() != candidate.draft_id()
            || current.draft().root_history() != published_pair
        {
            return Err(ComposerHostError::PublicationAssetMismatch.into());
        }
        if candidate.root().summary().logical_utf8_bytes() == 0
            && candidate.root().summary().marker_count() == 0
        {
            return Err(ComposerHostSubmissionError::Empty);
        }
        let owner = AssetOwner::CurrentDraft(candidate.draft_id());
        let asset_reference_set = match assets.owner_head(store, owner)? {
            Some(head) if head.owner() == owner => Some(head.set()),
            Some(_) => return Err(ComposerHostSubmissionError::AssetOwnerConflict),
            None => None,
        };
        if (candidate.root().summary().marker_count() == 0) != asset_reference_set.is_none() {
            return Err(ComposerHostSubmissionError::AssetOwnerConflict);
        }
        let gate = self
            .storage
            .input_gate(store, thread_id, submission_point_limit())?
            .ok_or(ComposerHostError::MissingCurrentDraft)?;
        let build = DraftComposerBuildKeyV1::new(
            candidate.root(),
            DraftComposerFormatV1::ComposerV1,
            request.materialization_operation_id(),
        );
        self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
            captured: CapturedSubmission {
                thread_id,
                candidate,
                thread_revision: current.thread().revision(),
                image_label_authority,
                draft_revision: current.draft().revision(),
                gate_revision: gate.revision(),
                gate_state: gate.state().clone(),
                asset_reference_set,
                build,
            },
            reconciliation: None,
        };
        Ok(ComposerHostSubmissionAdvance::Progress(
            ComposerHostSubmissionStage::Materializing,
        ))
    }

    fn advance_submission_materializer(
        &mut self,
        store: &HomeStore,
        captured: CapturedSubmission,
        reconciliation: Option<ReconciliationHandle>,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        if let Some(handle) = reconciliation {
            let resolution = store.reconcile(&handle).map_err(ComposerHostError::from)?;
            return match resolution {
                ReconciliationResolution::ExactOld | ReconciliationResolution::ExactNew { .. } => {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
                        captured,
                        reconciliation: None,
                    };
                    Ok(ComposerHostSubmissionAdvance::Progress(
                        ComposerHostSubmissionStage::Materializing,
                    ))
                }
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision => {
                    self.submission.pending = None;
                    Ok(ComposerHostSubmissionAdvance::Collision)
                }
            };
        }
        if cancellation.is_cancelled() {
            return self.cancel_submission();
        }
        #[cfg(feature = "test-faults")]
        self.inject_submission_transition_fault(ComposerHostSubmissionFaultPoint::Materializer)?;
        let status = self
            .storage
            .draft_composer_materialization_status(store, captured.build)?;
        match status {
            DraftComposerMaterializationStatusV1::Absent => {
                let mut command = HomeCommand::new(store.home_revision()?)
                    .with_cancellation(cancellation.clone());
                command.add(self.storage.begin_draft_composer_materialization(
                    self.storage.revision(store)?,
                    captured.build,
                ))?;
                self.settle_materializer_command(store.execute(command), captured, cancellation)
            }
            DraftComposerMaterializationStatusV1::Building(_) => {
                let Some(prepared) = self
                    .storage
                    .prepare_draft_composer_materialization_step(store, captured.build)?
                else {
                    self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
                        captured,
                        reconciliation: None,
                    };
                    return Ok(ComposerHostSubmissionAdvance::ReconciliationPending);
                };
                let mut command = HomeCommand::new(store.home_revision()?)
                    .with_cancellation(cancellation.clone());
                command.add(self.storage.advance_draft_composer_materialization(
                    self.storage.revision(store)?,
                    prepared,
                ))?;
                self.settle_materializer_command(store.execute(command), captured, cancellation)
            }
            DraftComposerMaterializationStatusV1::Sealed(materialization) => {
                let request = self.submission.pending.as_ref().unwrap().request;
                let acceptance = FirstAcceptance::new(
                    captured.thread_id,
                    captured.thread_revision,
                    captured.image_label_authority,
                    captured.candidate.draft_id(),
                    captured.draft_revision,
                    captured.candidate,
                    materialization,
                    captured.gate_revision,
                    captured.gate_state,
                    request.next_draft_id(),
                    request.idle_user_item_id(),
                    captured.asset_reference_set,
                    request.session_disposal_operation_id(),
                    request.admitted_at(),
                );
                self.pending_submission_mut().stage = PendingSubmissionStage::Accepting {
                    acceptance,
                    reconciliation: None,
                };
                Ok(ComposerHostSubmissionAdvance::Progress(
                    ComposerHostSubmissionStage::Accepting,
                ))
            }
            DraftComposerMaterializationStatusV1::Cancelled
            | DraftComposerMaterializationStatusV1::Failed(_)
            | DraftComposerMaterializationStatusV1::Superseded(_) => {
                Err(ComposerHostSubmissionError::MaterializationTerminal)
            }
        }
    }

    fn settle_materializer_command(
        &mut self,
        outcome: CommandOutcome,
        captured: CapturedSubmission,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        match outcome {
            CommandOutcome::Committed { .. } => {
                self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
                    captured,
                    reconciliation: None,
                };
                Ok(ComposerHostSubmissionAdvance::Progress(
                    ComposerHostSubmissionStage::Materializing,
                ))
            }
            CommandOutcome::NotCommitted { .. } if cancellation.is_cancelled() => {
                self.cancel_submission()
            }
            CommandOutcome::NotCommitted { .. } => {
                self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
                    captured,
                    reconciliation: None,
                };
                Ok(ComposerHostSubmissionAdvance::NotCommitted)
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                self.pending_submission_mut().stage = PendingSubmissionStage::Materializing {
                    captured,
                    reconciliation: Some(reconciliation.install_and_handle()),
                };
                Ok(ComposerHostSubmissionAdvance::ReconciliationPending)
            }
        }
    }

    fn cancel_submission(
        &mut self,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        self.submission.pending = None;
        Ok(ComposerHostSubmissionAdvance::Cancelled)
    }

    fn finish_submission_success(&mut self) {
        self.submission.pending = None;
        self.active = None;
        self.pending.clear();
        self.pending_mutation = None;
        self.pending_history = None;
        self.detached_mutations.clear();
        self.detached_history.clear();
        self.publication.lane = None;
        self.lifecycle.clear_runtime();
    }

    fn pending_submission_mut(&mut self) -> &mut PendingSubmission {
        self.submission
            .pending
            .as_mut()
            .expect("submission transition retains one pending coordinator")
    }

    #[cfg(feature = "test-faults")]
    fn inject_submission_transition_fault(
        &mut self,
        point: ComposerHostSubmissionFaultPoint,
    ) -> Result<(), ComposerHostSubmissionError> {
        if self.submission_transition_fault == Some(point) {
            self.submission_transition_fault = None;
            return Err(ComposerHostSubmissionError::InjectedFault(point));
        }
        Ok(())
    }

    #[cfg(feature = "test-faults")]
    fn inject_submission_cancellation_fault(
        &mut self,
        point: ComposerHostSubmissionFaultPoint,
        cancellation: &CommandCancellation,
    ) {
        if self.submission_transition_fault == Some(point) {
            self.submission_transition_fault = None;
            cancellation.cancel();
        }
    }
}

fn submission_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(64 * 1024).expect("submission point limit is nonzero")
}
