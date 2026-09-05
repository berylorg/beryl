use beryl_home_store::{CommandOutcome, HomeCommand, ReconciliationResolution};
use syndic_storage::{
    DraftEditorCandidateSessionAbandonFreshOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftRootHistoryPairV1,
};

use super::*;

impl MainWindowInitialComposer {
    pub fn retire(
        mut self,
        cancellation: CommandCancellation,
    ) -> MainWindowInitialComposerRetirement {
        self.candidate.retirement_started = true;
        match self.candidate.drive_retirement(cancellation) {
            Ok(true) => MainWindowInitialComposerRetirement::Retired(
                MainWindowShellUnpublished::from_retired_initial_composer(
                    self.acquisition,
                    self.reservation,
                ),
            ),
            Ok(false) => {
                MainWindowInitialComposerRetirement::Pending(MainWindowInitialComposerFailure {
                    custody: self,
                    error: "initial composer retirement remains pending".to_owned(),
                })
            }
            Err(error) => {
                MainWindowInitialComposerRetirement::Pending(MainWindowInitialComposerFailure {
                    custody: self,
                    error,
                })
            }
        }
    }
}

impl InitialComposerCandidate {
    pub(in crate::main_window) fn drive_retirement(
        &mut self,
        cancellation: CommandCancellation,
    ) -> Result<bool, String> {
        if !self.reconcile_open()? {
            return Ok(false);
        }
        if self.open_receipt.is_some() && self.opened.is_none() {
            self.classify_open()?;
        }
        let Some(head) = self.opened.as_ref() else {
            return Ok(true);
        };
        if self.abandonment.is_none() {
            let request = DraftEditorCandidateSessionDisposeRequestV1::new(
                head.draft_id(),
                head.session_id(),
                self.retirement_operation,
                head.session_generation(),
                DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
            );
            self.abandonment = Some(
                self.storage
                    .prepare_abandon_fresh_draft_editor_candidate_session(&self.store, request)
                    .map_err(|error| error.to_string())?,
            );
        }
        if let Some(handle) = self.abandonment_reconciliation.as_ref() {
            match self.store.retry_reconciliation(handle) {
                Err(_) => return Ok(false),
                Ok(ReconciliationResolution::ExactOld) => self.abandonment_reconciliation = None,
                Ok(ReconciliationResolution::ExactNew { receipt }) => {
                    self.abandonment_reconciliation = None;
                    self.abandonment_receipt = Some(receipt);
                }
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => {
                    return Err(
                        "initial composer retirement reconciliation collision retains custody"
                            .to_owned(),
                    );
                }
            }
        }
        if self.abandonment_receipt.is_none() {
            let mut command = HomeCommand::new(
                self.store
                    .home_revision()
                    .map_err(|error| error.to_string())?,
            )
            .with_cancellation(cancellation);
            command
                .add(
                    self.storage.abandon_fresh_draft_editor_candidate_session(
                        self.storage
                            .revision(&self.store)
                            .map_err(|error| error.to_string())?,
                        self.abandonment.as_ref().unwrap().clone(),
                    ),
                )
                .map_err(|error| error.to_string())?;
            #[cfg(feature = "test-faults")]
            if let Some(fault) = self.before_retirement.take() {
                fault(&self.store, self.storage.clone());
            }
            match self.store.execute(command) {
                outcome @ CommandOutcome::NotCommitted { .. } => {
                    return self.classify_abandonment(outcome);
                }
                CommandOutcome::Indeterminate { reconciliation, .. } => {
                    self.abandonment_reconciliation = Some(reconciliation.install_and_handle());
                    return Ok(false);
                }
                CommandOutcome::Committed { receipt, .. } => {
                    self.abandonment_receipt = Some(receipt)
                }
            }
        }
        self.classify_abandonment(CommandOutcome::Committed {
            receipt: self.abandonment_receipt.as_ref().unwrap().clone(),
            later_failure: None,
            local_finalization: None,
        })
    }

    fn classify_abandonment(&self, outcome: CommandOutcome) -> Result<bool, String> {
        let outcome = self
            .storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(
                &self.store,
                self.abandonment.as_ref().unwrap(),
                outcome,
            )
            .map_err(|error| error.to_string())?;
        match outcome {
            DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_)
            | DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(_) => Ok(true),
            DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
            | DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(_) => {
                Err(
                    "initial candidate departed the typed fresh-session retirement boundary"
                        .to_owned(),
                )
            }
        }
    }
}
