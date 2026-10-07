use beryl_home_store::{
    CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand, HomeHealthState,
    ReconciliationResolution,
};
use beryl_model::BerylHomeId;
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionAbandonFreshOutcomeV1,
    DraftEditorCandidateSessionCommandErrorV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftRootHistoryPairV1,
};

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MainWindowInitialComposerRecoveryProgress {
    Complete,
    Pending,
    Unavailable(String),
}

pub struct MainWindowInitialComposerRecoveryCleanup {
    home_id: BerylHomeId,
    canonical_home: std::path::PathBuf,
    failed_generation: HomeGeneration,
    opening: Option<PreparedDraftEditorCandidateSessionOpenV1>,
    disposal_operation: DraftPieceOperationIdV1,
    opening_reconciliation: Option<ReconciliationHandle>,
    opening_receipt: Option<CommitReceipt>,
    abandonment: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
    abandonment_reconciliation: Option<ReconciliationHandle>,
    abandonment_receipt: Option<CommitReceipt>,
    recovery_receipt: Option<CommitReceipt>,
    recovery_reconciliation: Option<ReconciliationHandle>,
    complete: bool,
    unavailable: Option<String>,
}

pub struct MainWindowInitialComposerRecovery {
    cleanup: MainWindowInitialComposerRecoveryCleanup,
    acquisition: RuntimeBackedWindowAcquisition,
    reservation: RuntimeBackedWindowMainWindowReservation,
}

impl MainWindowInitialComposerRecovery {
    pub fn cleanup_mut(&mut self) -> &mut MainWindowInitialComposerRecoveryCleanup {
        &mut self.cleanup
    }

    pub fn into_unpublished(self) -> Result<MainWindowShellUnpublished, Self> {
        if !self.cleanup.complete {
            return Err(self);
        }
        Ok(MainWindowShellUnpublished::from_retired_initial_composer(
            self.acquisition,
            self.reservation,
        ))
    }
}

impl MainWindowInitialComposer {
    pub fn capture_failed_recovery(
        self,
    ) -> Result<MainWindowInitialComposerRecovery, MainWindowInitialComposerFailure> {
        let Self {
            acquisition_store,
            acquisition_service,
            acquisition,
            reservation,
            candidate,
        } = self;
        match candidate.capture_failed_recovery() {
            Ok(cleanup) => Ok(MainWindowInitialComposerRecovery {
                cleanup,
                acquisition,
                reservation,
            }),
            Err((candidate, error)) => Err(MainWindowInitialComposerFailure {
                custody: Self {
                    acquisition_store,
                    acquisition_service,
                    acquisition,
                    reservation,
                    candidate,
                },
                error,
            }),
        }
    }
}

impl InitialComposerCandidate {
    pub(in crate::main_window) fn capture_failed_recovery(
        mut self,
    ) -> Result<MainWindowInitialComposerRecoveryCleanup, (Self, String)> {
        let health = self.store.health();
        if health.state() != HomeHealthState::Failed
            || health.generation() != Some(self.home_generation)
        {
            return Err((
                self,
                "initial composer requires its exact failed home generation".into(),
            ));
        }
        if self.service.is_some() {
            return Err((
                self,
                "initial composer prepared service has not retired".into(),
            ));
        }
        if let Some(host) = self.host.as_ref() {
            if host.pending_request_count() != 0 || host.settlement_custody_in_use() != 0 {
                return Err((self, "initial composer host retains admitted work".into()));
            }
            if let Some(binding) = host.binding() {
                if self.opened.as_ref().is_none_or(|head| {
                    binding.candidate() != DraftEditorCandidateActivationBindingV1::from_head(head)
                }) || host
                    .fresh_abandonment_request(self.retirement_operation)
                    .is_none()
                {
                    return Err((
                        self,
                        "initial composer host is not the original fresh editor".into(),
                    ));
                }
                let host = Box::new(self.host.take().unwrap());
                match host.retire_failed_resident(&self.store) {
                    Ok(retired) => drop(retired),
                    Err(host) => {
                        self.host = Some(*host);
                        return Err((
                            self,
                            "initial composer host retirement remains pending".into(),
                        ));
                    }
                }
            }
        }
        Ok(MainWindowInitialComposerRecoveryCleanup {
            home_id: self.store.home_id(),
            canonical_home: self.store.canonical_path().to_owned(),
            failed_generation: self.home_generation,
            opening: self.open,
            disposal_operation: self.retirement_operation,
            opening_reconciliation: self.open_reconciliation,
            opening_receipt: self.open_receipt,
            abandonment: self.abandonment,
            abandonment_reconciliation: self.abandonment_reconciliation,
            abandonment_receipt: self.abandonment_receipt,
            recovery_receipt: None,
            recovery_reconciliation: None,
            complete: false,
            unavailable: None,
        })
    }
}

impl MainWindowInitialComposerRecoveryCleanup {
    #[cfg(feature = "test-faults")]
    pub fn test_original_opening(&self) -> Option<&PreparedDraftEditorCandidateSessionOpenV1> {
        self.opening.as_ref()
    }

    pub fn settle(
        &mut self,
        storage: &SyndicStorage,
        access: &mut HomeCandidateRecoveryAccess<'_>,
        cancellation: CommandCancellation,
    ) -> Result<MainWindowInitialComposerRecoveryProgress, String> {
        if access.home_id() != self.home_id
            || access.canonical_path() != self.canonical_home
            || access.generation() == self.failed_generation
        {
            return Err(
                "initial composer recovery requires a fresh candidate for the same home".into(),
            );
        }
        if let Some(reason) = &self.unavailable {
            return Ok(MainWindowInitialComposerRecoveryProgress::Unavailable(
                reason.clone(),
            ));
        }
        self.complete = false;
        let mut collision = false;
        for (handle, receipt) in [
            (&mut self.opening_reconciliation, &mut self.opening_receipt),
            (
                &mut self.abandonment_reconciliation,
                &mut self.abandonment_receipt,
            ),
            (
                &mut self.recovery_reconciliation,
                &mut self.recovery_receipt,
            ),
        ] {
            let Some(pending) = handle.as_ref() else {
                continue;
            };
            match access.retry_reconciliation(pending) {
                Err(_) => return Ok(MainWindowInitialComposerRecoveryProgress::Pending),
                Ok(ReconciliationResolution::ExactOld) => {
                    *handle = None;
                    *receipt = None;
                }
                Ok(ReconciliationResolution::ExactNew { receipt: exact }) => {
                    *handle = None;
                    *receipt = Some(exact);
                }
                Ok(
                    ReconciliationResolution::Collision
                    | ReconciliationResolution::ExactSuccessor { .. },
                ) => {
                    collision = true;
                    break;
                }
            }
        }
        if collision {
            return Ok(self.mark_unavailable("initial composer recovery reconciliation collision"));
        }
        let Some(opening) = self.opening.as_ref() else {
            if self.opening_receipt.is_some() || self.abandonment.is_some() {
                return Ok(
                    self.mark_unavailable("initial composer recovery lacks its original opening")
                );
            }
            self.complete = true;
            return Ok(MainWindowInitialComposerRecoveryProgress::Complete);
        };
        if let Some(receipt) = &self.opening_receipt {
            let outcome = storage
                .reconcile_draft_editor_candidate_session_open_candidate(
                    access,
                    opening,
                    committed(receipt),
                )
                .map_err(|error| error.to_string())?;
            if !matches!(
                outcome,
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(_)
                    | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(_)
                    | DraftEditorCandidateSessionOpenOutcomeV1::StaleDisposed(_)
            ) {
                return Ok(self.mark_unavailable("initial composer recovery opening is not exact"));
            }
        }
        let qualified = match storage.qualify_fresh_draft_editor_candidate_session_open_candidate(
            access,
            self.home_id,
            self.disposal_operation,
            opening,
        ) {
            Ok(qualified) => qualified,
            Err(DraftEditorCandidateSessionCommandErrorV1::Invariant) => {
                return Ok(
                    self.mark_unavailable("initial composer recovery original intent is not exact")
                );
            }
            Err(error) => return Err(error.to_string()),
        };
        match qualified {
            DraftEditorCandidateSessionReadOutcomeV1::Absent => {
                if self.opening_receipt.is_some()
                    || self.abandonment_receipt.is_some()
                    || self.recovery_receipt.is_some()
                {
                    return Ok(self.mark_unavailable(
                        "initial composer recovery contradicts committed outcome",
                    ));
                }
                self.complete = true;
                return Ok(MainWindowInitialComposerRecoveryProgress::Complete);
            }
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => {
                if head.disposal_operation_id() != Some(self.disposal_operation) {
                    return Ok(self
                        .mark_unavailable("initial composer recovery disposal identity differs"));
                }
                if let Some(prepared) = &self.abandonment {
                    if let Some(receipt) = self
                        .recovery_receipt
                        .as_ref()
                        .or(self.abandonment_receipt.as_ref())
                    {
                        self.classify_abandonment(storage, access, prepared, receipt)?;
                    }
                }
                self.complete = true;
                return Ok(MainWindowInitialComposerRecoveryProgress::Complete);
            }
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => {
                if self.abandonment_receipt.is_some() || self.recovery_receipt.is_some() {
                    return Ok(self.mark_unavailable(
                        "initial composer recovery disposal contradicts active session",
                    ));
                }
                if cancellation.is_cancelled() {
                    return Ok(MainWindowInitialComposerRecoveryProgress::Pending);
                }
                if self.abandonment.is_none() {
                    self.abandonment = Some(
                        storage
                            .prepare_abandon_fresh_draft_editor_candidate_session_candidate(
                                access,
                                DraftEditorCandidateSessionDisposeRequestV1::new(
                                    head.draft_id(),
                                    head.session_id(),
                                    self.disposal_operation,
                                    head.session_generation(),
                                    DraftRootHistoryPairV1::new(
                                        head.newest_root(),
                                        head.newest_history(),
                                    ),
                                ),
                            )
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
                return Ok(MainWindowInitialComposerRecoveryProgress::Pending);
            }
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
                return Ok(
                    self.mark_unavailable("initial composer recovery opening invariant failed")
                );
            }
        }
        let mut command =
            HomeCommand::new(access.home_revision().map_err(|error| error.to_string())?)
                .with_cancellation(cancellation);
        command
            .add(
                storage.abandon_fresh_draft_editor_candidate_session(
                    storage
                        .revision_candidate(access)
                        .map_err(|error| error.to_string())?,
                    self.abandonment.as_ref().unwrap().clone(),
                ),
            )
            .map_err(|error| error.to_string())?;
        match access.execute(command) {
            CommandOutcome::NotCommitted { .. } => {
                Ok(MainWindowInitialComposerRecoveryProgress::Pending)
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                self.recovery_reconciliation = Some(reconciliation.install_and_handle());
                Ok(MainWindowInitialComposerRecoveryProgress::Pending)
            }
            CommandOutcome::Committed { receipt, .. } => {
                self.recovery_receipt = Some(receipt);
                self.classify_abandonment(
                    storage,
                    access,
                    self.abandonment.as_ref().unwrap(),
                    self.recovery_receipt.as_ref().unwrap(),
                )?;
                self.complete = true;
                Ok(MainWindowInitialComposerRecoveryProgress::Complete)
            }
        }
    }

    fn mark_unavailable(&mut self, reason: &str) -> MainWindowInitialComposerRecoveryProgress {
        self.unavailable = Some(reason.into());
        MainWindowInitialComposerRecoveryProgress::Unavailable(reason.into())
    }

    fn classify_abandonment(
        &self,
        storage: &SyndicStorage,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
        receipt: &CommitReceipt,
    ) -> Result<(), String> {
        let outcome = storage
            .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
                access,
                prepared,
                committed(receipt),
            )
            .map_err(|error| error.to_string())?;
        match outcome {
            DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
            | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_)
            | DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(_) => Ok(()),
            _ => Err("initial composer recovery abandonment is not exact".into()),
        }
    }
}

fn committed(receipt: &CommitReceipt) -> CommandOutcome {
    CommandOutcome::Committed {
        receipt: receipt.clone(),
        later_failure: None,
        local_finalization: None,
    }
}
