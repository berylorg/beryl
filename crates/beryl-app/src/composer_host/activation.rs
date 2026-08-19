use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCommand, HomeHealthState, HomeStore,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftPieceTextDemandV1,
};

use super::*;

impl SyndicComposerHost {
    pub fn activate(
        &mut self,
        store: &HomeStore,
        request: ComposerHostActivationRequest,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostActivationOutcome, ComposerHostError> {
        match &self.pending_mutation {
            Some(super::ComposerHostPendingMutation::Admitted(_)) => {
                return Err(ComposerHostError::MutationCustodyPending);
            }
            Some(super::ComposerHostPendingMutation::Unavailable(_)) => {
                return Err(ComposerHostError::MutationUnavailable);
            }
            Some(super::ComposerHostPendingMutation::Staged(_)) | None => {}
        }
        if request.first_demands().len() > COMPOSER_HOST_MAX_INITIAL_DEMANDS {
            return Err(ComposerHostError::TooManyInitialDemands);
        }
        validate_initial_request_order(request.first_demands())?;
        if cancellation.is_cancelled() {
            return Ok(ComposerHostActivationOutcome::Cancelled);
        }
        let health = store.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ComposerHostError::HomeUnavailable(health.state()));
        }
        let home_generation = health
            .generation()
            .ok_or(ComposerHostError::HomeUnavailable(health.state()))?;
        let selector_probe = self
            .storage
            .current_draft_piece_text_demand(
                store,
                request.thread_id(),
                DraftPieceTextDemandV1::Validate(0),
                4,
            )?
            .ok_or(ComposerHostError::MissingCurrentDraft)?;
        let selector = selector_probe.selector();
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.activation_after_selector_fault.take() {
            fault(store, self.storage);
        }
        let open_request = DraftEditorCandidateSessionOpenRequestV1::new(
            selector,
            request.session_id(),
            request.operation_id(),
        );
        let prepared = self
            .storage
            .prepare_open_draft_editor_candidate_session(store, open_request)?;
        if cancellation.is_cancelled() {
            return Ok(ComposerHostActivationOutcome::Cancelled);
        }
        let mut command =
            HomeCommand::new(store.home_revision()?).with_cancellation(cancellation.clone());
        command.add(self.storage.open_draft_editor_candidate_session(
            self.storage.revision(store)?,
            prepared.clone(),
        ))?;
        let command_outcome = store.execute(command);
        if matches!(
            &command_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::CancelledBeforeAdmission
            }
        ) {
            return Ok(ComposerHostActivationOutcome::Cancelled);
        }
        let open_outcome = self.storage.reconcile_draft_editor_candidate_session_open(
            store,
            &prepared,
            command_outcome,
        )?;
        let (disposition, head) = match open_outcome {
            DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => {
                (ComposerHostOpenDisposition::Opened, head)
            }
            DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => {
                (ComposerHostOpenDisposition::ExactReplay, head)
            }
            DraftEditorCandidateSessionOpenOutcomeV1::StaleDisposed(head) => {
                return Ok(ComposerHostActivationOutcome::StaleDisposed(head));
            }
            DraftEditorCandidateSessionOpenOutcomeV1::SelectorConflict(selector) => {
                return Ok(ComposerHostActivationOutcome::SelectorConflict(selector));
            }
            DraftEditorCandidateSessionOpenOutcomeV1::OccupiedIdentityCollision(proof) => {
                return Ok(ComposerHostActivationOutcome::OccupiedIdentityCollision(
                    proof,
                ));
            }
        };
        let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
        let host_generation = self.next_generation()?;
        let binding = ComposerHostBinding::new(
            store.home_id(),
            home_generation,
            host_generation,
            candidate,
            request.presentation_generation(),
        );
        if let Some(restoration) = request.restoration() {
            if restoration.root() != binding.root()
                || restoration.logical_extent() != binding.logical_extent()
            {
                return Err(ComposerHostError::RestorationBindingMismatch);
            }
            self.storage
                .validate_draft_piece_restoration(store, restoration.restoration())?;
        }
        let mut initial_responses = Vec::with_capacity(request.first_demands().len());
        for demand in request.first_demands() {
            if cancellation.is_cancelled() {
                return Ok(ComposerHostActivationOutcome::Cancelled);
            }
            let key = ComposerHostRequestKey::new(binding, demand.request_id(), demand.purpose());
            let value = match demand {
                ComposerHostInitialDemand::Text {
                    demand, max_bytes, ..
                } => ComposerHostResponseValue::CandidateText(
                    self.storage
                        .candidate_draft_piece_text_demand(store, candidate, *demand, *max_bytes)?,
                ),
                ComposerHostInitialDemand::Markers { demand, .. } => {
                    ComposerHostResponseValue::CandidateMarkers(
                        self.storage.candidate_draft_piece_marker_demand(
                            store,
                            candidate,
                            demand.clone(),
                        )?,
                    )
                }
                ComposerHostInitialDemand::MarkerProof {
                    request,
                    retained_byte_ceiling,
                    ..
                } => ComposerHostResponseValue::CandidateMarkerProof(
                    self.storage.candidate_draft_piece_marker_edge_proof(
                        store,
                        candidate,
                        *request,
                        *retained_byte_ceiling,
                    )?,
                ),
            };
            initial_responses.push(ComposerHostResponse::new(key, value));
        }
        if cancellation.is_cancelled() {
            return Ok(ComposerHostActivationOutcome::Cancelled);
        }
        let after = store.health();
        if after.state() != HomeHealthState::Healthy {
            return Err(ComposerHostError::HomeUnavailable(after.state()));
        }
        if after.generation() != Some(home_generation) {
            return Err(ComposerHostError::HomeGenerationChanged {
                expected: home_generation,
                actual: after.generation(),
            });
        }
        self.active = Some(ActiveComposerHost {
            binding,
            storage_candidate: binding.candidate(),
            thread_id: request.thread_id(),
            initial_responses,
        });
        self.last_generation = Some(host_generation);
        self.last_request_id = request
            .first_demands()
            .last()
            .map_or(0, |demand| demand.request_id().get());
        self.pending.clear();
        self.pending_mutation = None;
        Ok(ComposerHostActivationOutcome::Activated {
            disposition,
            binding,
        })
    }
}

fn validate_initial_request_order(
    demands: &[ComposerHostInitialDemand],
) -> Result<(), ComposerHostError> {
    let mut previous = 0;
    for demand in demands {
        let current = demand.request_id().get();
        if current <= previous {
            return Err(ComposerHostError::InvalidInitialRequestOrder);
        }
        previous = current;
    }
    Ok(())
}
