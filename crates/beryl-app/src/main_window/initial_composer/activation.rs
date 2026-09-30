use beryl_home_store::{CommandOutcome, HomeCommand, ReconciliationResolution};
use syndic_storage::{
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceTextDemandV1,
};

use super::*;
use crate::composer_host::ComposerHostActivationOutcome;
use crate::main_window::{
    MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
    MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
};

impl MainWindowInitialComposer {
    pub fn advance(
        &mut self,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowInitialComposerProgress, String> {
        let service = &self.acquisition_service;
        let acquisition = &self.acquisition;
        let store = self.acquisition_store.clone();
        let generation = self.candidate.home_generation;
        let claim = self.candidate.claim;
        self.candidate.advance(
            cancellation,
            acquisition.thread_id(),
            acquisition.draft_id(),
            &|| {
                if store.health().generation() != Some(generation) {
                    return Err("initial composer home generation changed".to_owned());
                }
                service.validate_initial_composer_claim(acquisition, claim, &store)
            },
        )
    }

    pub fn prepare(
        mut self,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
    ) -> Result<MainWindowInitialComposerPrepared, MainWindowInitialComposerFailure> {
        let service = &self.acquisition_service;
        let acquisition = &self.acquisition;
        let store = self.acquisition_store.clone();
        let generation = self.candidate.home_generation;
        let claim = self.candidate.claim;
        let result = self
            .candidate
            .prepare_selection(acquisition.window_id(), configurator, &|| {
                if store.health().generation() != Some(generation) {
                    return Err("initial composer home generation changed".to_owned());
                }
                service.validate_initial_composer_claim(acquisition, claim, &store)
            })
            .and_then(|prepared| {
                service.validate_shell_selection(acquisition, prepared.selection_identity())?;
                Ok(prepared)
            });
        match result {
            Ok(prepared) => Ok(MainWindowInitialComposerPrepared {
                prepared,
                custody: self,
            }),
            Err(error) => Err(MainWindowInitialComposerFailure {
                custody: self,
                error,
            }),
        }
    }
}

impl InitialComposerCandidate {
    pub(super) fn reconcile_open(&mut self) -> Result<bool, String> {
        let Some(handle) = self.open_reconciliation.as_ref() else {
            return Ok(true);
        };
        match self.store.retry_reconciliation(handle) {
            Err(_) => Ok(false),
            Ok(ReconciliationResolution::ExactOld) => {
                self.open_reconciliation = None;
                self.open_receipt = None;
                Ok(true)
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                self.open_reconciliation = None;
                self.open_receipt = Some(receipt);
                Ok(true)
            }
            Ok(
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision,
            ) => Err("initial composer open reconciliation collision retains custody".to_owned()),
        }
    }

    pub(super) fn classify_open(&mut self) -> Result<(), String> {
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.before_open_classification.take() {
            fault(&self.store, self.storage.clone());
        }
        let outcome = self
            .storage
            .reconcile_draft_editor_candidate_session_open(
                &self.store,
                self.open.as_ref().unwrap(),
                CommandOutcome::Committed {
                    receipt: self.open_receipt.as_ref().unwrap().clone(),
                    later_failure: None,
                    local_finalization: None,
                },
            )
            .map_err(|error| error.to_string())?;
        match outcome {
            DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
            | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => {
                self.opened = Some(head);
                Ok(())
            }
            DraftEditorCandidateSessionOpenOutcomeV1::StaleDisposed(head) => {
                self.opened = Some(head);
                self.open_terminal = true;
                Err("initial composer candidate was already disposed".to_owned())
            }
            other => Err(format!(
                "initial composer open is not the exact active candidate: {other:?}"
            )),
        }
    }
}

impl InitialComposerCandidate {
    pub(super) fn advance(
        &mut self,
        cancellation: &CommandCancellation,
        thread_id: beryl_model::SyndicThreadId,
        draft_id: beryl_model::SyndicDraftId,
        validate_source: &impl Fn() -> Result<(), String>,
    ) -> Result<MainWindowInitialComposerProgress, String> {
        if self.retirement_started || self.preparation_started || self.open_terminal {
            return Err(
                "initial composer activation has already transferred or retired".to_owned(),
            );
        }
        validate_source()?;
        if self.activated {
            return Ok(MainWindowInitialComposerProgress::Activated);
        }
        if self.open_reconciliation.is_some() {
            if !self.reconcile_open()? {
                return Ok(MainWindowInitialComposerProgress::Pending);
            }
        }
        if self.open_receipt.is_some() && self.opened.is_none() {
            self.classify_open()?;
        }
        if self.opened.is_none() {
            if cancellation.is_cancelled() {
                return Err("initial composer activation cancelled".to_owned());
            }
            if self.open.is_none() {
                let probe = self
                    .storage
                    .current_draft_piece_text_demand(
                        &self.store,
                        thread_id,
                        DraftPieceTextDemandV1::Validate(0),
                        4,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "initial composer draft is missing".to_owned())?;
                if probe.selector().draft_id() != draft_id {
                    return Err("initial composer selector differs from acquisition".to_owned());
                }
                let occupied = self
                    .storage
                    .draft_editor_candidate_session(
                        &self.store,
                        draft_id,
                        self.request.session_id(),
                    )
                    .map_err(|error| error.to_string())?;
                if !matches!(occupied, DraftEditorCandidateSessionReadOutcomeV1::Absent) {
                    return Err(
                        "initial composer candidate identity is already occupied".to_owned()
                    );
                }
                self.open = Some(
                    self.storage
                        .prepare_open_draft_editor_candidate_session(
                            &self.store,
                            DraftEditorCandidateSessionOpenRequestV1::new(
                                probe.selector(),
                                self.request.session_id(),
                                self.request.operation_id(),
                            ),
                        )
                        .map_err(|error| error.to_string())?,
                );
            }
            validate_source()?;
            let mut command = HomeCommand::new(
                self.store
                    .home_revision()
                    .map_err(|error| error.to_string())?,
            )
            .with_cancellation(cancellation.clone());
            command
                .add(
                    self.storage.open_draft_editor_candidate_session(
                        self.storage
                            .revision(&self.store)
                            .map_err(|error| error.to_string())?,
                        self.open.as_ref().unwrap().clone(),
                    ),
                )
                .map_err(|error| error.to_string())?;
            #[cfg(feature = "test-faults")]
            if let Some(fault) = self.before_open.take() {
                fault(&self.store, self.storage.clone());
            }
            match self.store.execute(command) {
                CommandOutcome::NotCommitted { .. } => {
                    return Ok(MainWindowInitialComposerProgress::Retry);
                }
                CommandOutcome::Indeterminate { reconciliation, .. } => {
                    self.open_reconciliation = Some(reconciliation.install_and_handle());
                    return Ok(MainWindowInitialComposerProgress::Pending);
                }
                CommandOutcome::Committed { receipt, .. } => self.open_receipt = Some(receipt),
            }
            self.classify_open()?;
        }
        validate_source()?;
        let result = self
            .host
            .as_mut()
            .ok_or_else(|| "initial composer host was already consumed".to_owned())?
            .finish_initial_activation(
                &self.store,
                self.request.clone(),
                cancellation,
                self.home_generation,
                self.open.as_ref().unwrap().request().selector(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(
                    self.opened.as_ref().unwrap().clone(),
                ),
                true,
            )
            .map_err(|error| error.to_string())?;
        if !matches!(result, ComposerHostActivationOutcome::Activated { .. }) {
            return Err(format!(
                "initial composer activation did not become ready: {result:?}"
            ));
        }
        self.activated = true;
        validate_source()?;
        Ok(MainWindowInitialComposerProgress::Activated)
    }
}
impl InitialComposerCandidate {
    pub(super) fn prepare_selection(
        &mut self,
        window_id: beryl_model::WindowId,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
        validate_source: &impl Fn() -> Result<(), String>,
    ) -> Result<MainWindowConversationComposerPreparedSelection, String> {
        validate_source()?;
        if !self.activated || self.retirement_started || self.preparation_started {
            return Err(
                "initial composer is not available for selected-editor preparation".to_owned(),
            );
        }
        self.preparation_started = true;
        let service = self.prepare_selection_service(window_id)?;
        let prepared =
            MainWindowConversationComposerMount::prepare_selected(service, configurator)?;
        validate_source()?;
        Ok(prepared)
    }

    #[inline(never)]
    fn prepare_selection_service(
        &mut self,
        window_id: beryl_model::WindowId,
    ) -> Result<Arc<MainWindowConversationComposerService>, String> {
        let slot = MainWindowComposerSlot::new(
            window_id,
            self.claim,
            self.host.take().unwrap(),
            self.storage.clone(),
            self.marker_authority.take().unwrap(),
        )
        .map_err(|error| error.to_string())?;
        let service = Arc::new(MainWindowConversationComposerService::new(
            self.store.service_reference(),
            slot,
        ));
        self.service = Some(service.clone());
        Ok(service)
    }
}
