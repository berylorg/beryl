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
        if self.candidate.retirement_started
            || self.candidate.preparation_started
            || self.candidate.open_terminal
        {
            return Err(
                "initial composer activation has already transferred or retired".to_owned(),
            );
        }
        self.validate_source()?;
        if self.candidate.activated {
            return Ok(MainWindowInitialComposerProgress::Activated);
        }
        if self.candidate.open_reconciliation.is_some() {
            if !self.candidate.reconcile_open()? {
                return Ok(MainWindowInitialComposerProgress::Pending);
            }
        }
        if self.candidate.open_receipt.is_some() && self.candidate.opened.is_none() {
            self.candidate.classify_open()?;
        }
        if self.candidate.opened.is_none() {
            if cancellation.is_cancelled() {
                return Err("initial composer activation cancelled".to_owned());
            }
            if self.candidate.open.is_none() {
                let probe = self
                    .candidate
                    .storage
                    .current_draft_piece_text_demand(
                        &self.candidate.store,
                        self.acquisition.thread_id(),
                        DraftPieceTextDemandV1::Validate(0),
                        4,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "initial composer draft is missing".to_owned())?;
                if probe.selector().draft_id() != self.acquisition.draft_id() {
                    return Err("initial composer selector differs from acquisition".to_owned());
                }
                let occupied = self
                    .candidate
                    .storage
                    .draft_editor_candidate_session(
                        &self.candidate.store,
                        self.acquisition.draft_id(),
                        self.candidate.request.session_id(),
                    )
                    .map_err(|error| error.to_string())?;
                if !matches!(occupied, DraftEditorCandidateSessionReadOutcomeV1::Absent) {
                    return Err(
                        "initial composer candidate identity is already occupied".to_owned()
                    );
                }
                self.candidate.open = Some(
                    self.candidate
                        .storage
                        .prepare_open_draft_editor_candidate_session(
                            &self.candidate.store,
                            DraftEditorCandidateSessionOpenRequestV1::new(
                                probe.selector(),
                                self.candidate.request.session_id(),
                                self.candidate.request.operation_id(),
                            ),
                        )
                        .map_err(|error| error.to_string())?,
                );
            }
            self.validate_source()?;
            let mut command = HomeCommand::new(
                self.candidate
                    .store
                    .home_revision()
                    .map_err(|error| error.to_string())?,
            )
            .with_cancellation(cancellation.clone());
            command
                .add(
                    self.candidate.storage.open_draft_editor_candidate_session(
                        self.candidate
                            .storage
                            .revision(&self.candidate.store)
                            .map_err(|error| error.to_string())?,
                        self.candidate.open.as_ref().unwrap().clone(),
                    ),
                )
                .map_err(|error| error.to_string())?;
            #[cfg(feature = "test-faults")]
            if let Some(fault) = self.candidate.before_open.take() {
                fault(&self.candidate.store, self.candidate.storage.clone());
            }
            match self.candidate.store.execute(command) {
                CommandOutcome::NotCommitted { .. } => {
                    return Ok(MainWindowInitialComposerProgress::Retry);
                }
                CommandOutcome::Indeterminate { reconciliation, .. } => {
                    self.candidate.open_reconciliation = Some(reconciliation.install_and_handle());
                    return Ok(MainWindowInitialComposerProgress::Pending);
                }
                CommandOutcome::Committed { receipt, .. } => {
                    self.candidate.open_receipt = Some(receipt)
                }
            }
            self.candidate.classify_open()?;
        }
        self.validate_source()?;
        let result = self
            .candidate
            .host
            .as_mut()
            .ok_or_else(|| "initial composer host was already consumed".to_owned())?
            .finish_initial_activation(
                &self.candidate.store,
                self.candidate.request.clone(),
                cancellation,
                self.candidate.home_generation,
                self.candidate.open.as_ref().unwrap().request().selector(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(
                    self.candidate.opened.as_ref().unwrap().clone(),
                ),
                true,
            )
            .map_err(|error| error.to_string())?;
        if !matches!(result, ComposerHostActivationOutcome::Activated { .. }) {
            return Err(format!(
                "initial composer activation did not become ready: {result:?}"
            ));
        }
        self.candidate.activated = true;
        self.validate_source()?;
        Ok(MainWindowInitialComposerProgress::Activated)
    }

    pub fn prepare(
        mut self,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
    ) -> Result<MainWindowInitialComposerPrepared, MainWindowInitialComposerFailure> {
        let result = (|| {
            self.validate_source()?;
            if !self.candidate.activated
                || self.candidate.retirement_started
                || self.candidate.preparation_started
            {
                return Err(
                    "initial composer is not available for selected-editor preparation".to_owned(),
                );
            }
            self.candidate.preparation_started = true;
            let slot = MainWindowComposerSlot::new(
                self.acquisition.window_id(),
                self.candidate.claim,
                self.candidate.host.take().unwrap(),
                self.candidate.storage.clone(),
                self.candidate.marker_authority.take().unwrap(),
            )
            .map_err(|error| error.to_string())?;
            let service = Arc::new(MainWindowConversationComposerService::new(
                self.candidate.store.clone(),
                slot,
            ));
            self.candidate.service = Some(service.clone());
            let prepared =
                MainWindowConversationComposerMount::prepare_selected(service, configurator)?;
            self.candidate
                .acquisition_service
                .validate_shell_selection(&self.acquisition, prepared.selection_identity())?;
            self.validate_source()?;
            Ok(prepared)
        })();
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
