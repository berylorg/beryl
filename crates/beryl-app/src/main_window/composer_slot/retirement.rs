use beryl_home_store::HomeCommand;
use syndic_storage::DraftEditorCandidateSessionAbandonFreshOutcomeV1;

use super::*;
use crate::composer_host::ComposerHostServiceDisposalCompletion;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn disposal_flush_is_current(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
    ) -> bool {
        self.selected_identity() == Some(selection)
            && matches!(self.disposal_stage,
                Some(DisposalStage::Flushing(current) | DisposalStage::AwaitingWidgetRelease(current))
                    if current == flush)
    }

    pub fn release_failed_pending(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if !self.pending_source_is_current(store, receipt)? {
            return self.dispose_failed_pending(store);
        }
        match self.retire_pending(store, receipt) {
            Ok(advance) => Ok(advance),
            Err(MainWindowComposerSlotError::TargetNotFresh) => self.dispose_failed_pending(store),
            Err(error) => Err(error),
        }
    }

    fn dispose_failed_pending(
        &mut self,
        store: &HomeStore,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        let pending = self.pending.as_mut().unwrap();
        match pending.host.dispose_composer_service(store) {
            Err(crate::composer_host::ComposerHostError::PublicationPending) => {
                Ok(MainWindowComposerRetirementAdvance::Pending)
            }
            Err(error) => Err(error.into()),
            Ok(ComposerHostServiceDisposalCompletion::Pending) => {
                Ok(MainWindowComposerRetirementAdvance::Pending)
            }
            Ok(ComposerHostServiceDisposalCompletion::Disposed) => {
                self.pending = None;
                self.submission_successor = None;
                Ok(MainWindowComposerRetirementAdvance::Retired)
            }
        }
    }

    pub fn retire_pending(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if matches!(
            self.pending.as_ref().unwrap().stage,
            PendingStage::Publishing(_)
                | PendingStage::SelectionSaving(_)
                | PendingStage::SelectionSaved(_)
                | PendingStage::SelectionDisposing(_)
                | PendingStage::AwaitingWidgetRelease
                | PendingStage::Finalizing
        ) {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        self.pending.as_mut().unwrap().stage = PendingStage::Retiring;
        self.drive_retirement(store, receipt)
    }

    pub fn reconcile_pending_after_recovery(
        &mut self,
        candidate: &mut beryl_home_store::HomeRecoveryCandidate,
        recovered_storage: SyndicStorage,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let pending = self.pending.as_ref().unwrap();
        if !matches!(pending.stage, PendingStage::Reconciliation) || pending.abandonment.is_none() {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        let access = candidate
            .recovery_access()
            .map_err(|_| MainWindowComposerSlotError::RecoveryHandleMismatch)?;
        recovered_storage
            .revision_candidate(&access)
            .map_err(|_| MainWindowComposerSlotError::RecoveryHandleMismatch)?;
        let pending = self.pending.as_mut().unwrap();
        let binding = pending
            .host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        if pending.retain_thread_cleanup
            || access.home_id() != binding.home_id()
            || access.generation() == binding.home_generation()
            || pending.abandonment_canonical_home.as_deref() != Some(access.canonical_path())
        {
            return Err(MainWindowComposerSlotError::RecoveryHandleMismatch);
        }
        let Some(original) = pending.abandonment_outcome.as_mut() else {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        };
        match original.reconcile(&access) {
            Ok(true) => {}
            Ok(false) => {
                let mut command = HomeCommand::new(
                    access
                        .home_revision()
                        .map_err(MainWindowComposerSlotError::RecoveryRead)?,
                );
                command
                    .add(
                        recovered_storage.abandon_fresh_draft_editor_candidate_session(
                            recovered_storage
                                .revision_candidate(&access)
                                .map_err(MainWindowComposerSlotError::RecoveryRead)?,
                            pending.abandonment.as_ref().unwrap().clone(),
                        ),
                    )
                    .map_err(|_| MainWindowComposerSlotError::TargetNotReady)?;
                *original = crate::composer_host::RetainedComposerCommandOutcome::new(
                    access.execute(command),
                );
                if original.reconcile(&access) != Ok(true) {
                    return Ok(MainWindowComposerRetirementAdvance::Pending);
                }
            }
            Err(_) => return Ok(MainWindowComposerRetirementAdvance::Pending),
        }
        match recovered_storage.reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            &access,
            pending.abandonment.as_ref().unwrap(),
            original.committed_classification().unwrap(),
        ) {
            Ok(
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
                | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_),
            ) => {
                self.pending = None;
                self.submission_successor = None;
                self.storage = recovered_storage;
                Ok(MainWindowComposerRetirementAdvance::Retired)
            }
            Ok(_) => {
                pending.stage = PendingStage::Departed;
                Ok(MainWindowComposerRetirementAdvance::DepartedFreshBoundary)
            }
            Err(_) => Ok(MainWindowComposerRetirementAdvance::Pending),
        }
    }

    pub fn begin_disposal(
        &mut self,
        store: &HomeStore,
    ) -> Result<ComposerHostFlushAdmission, MainWindowComposerSlotError> {
        self.ensure_live()?;
        if let Some(receipt) = self.pending_receipt() {
            match self.retire_pending(store, receipt)? {
                MainWindowComposerRetirementAdvance::Retired => {}
                MainWindowComposerRetirementAdvance::Pending => {
                    return Err(MainWindowComposerSlotError::ActivationPending);
                }
                MainWindowComposerRetirementAdvance::DepartedFreshBoundary => {
                    return Err(MainWindowComposerSlotError::TargetNotFresh);
                }
            }
        }
        let admission = self
            .selected
            .as_mut()
            .ok_or(MainWindowComposerSlotError::Disposed)?
            .host
            .begin_flush(ComposerHostFlushPurpose::Release)?;
        match admission {
            ComposerHostFlushAdmission::Started { ticket, .. }
            | ComposerHostFlushAdmission::Joined { ticket, .. } => {
                self.disposal_stage = Some(DisposalStage::Flushing(ticket))
            }
            ComposerHostFlushAdmission::Satisfied(_) => {
                return Err(MainWindowComposerSlotError::StaleActivationReceipt);
            }
        }
        Ok(admission)
    }

    pub fn advance_disposal(
        &mut self,
        store: &HomeStore,
    ) -> Result<MainWindowComposerDisposalAdvance, MainWindowComposerSlotError> {
        let ticket = match self
            .disposal_stage
            .ok_or(MainWindowComposerSlotError::TargetNotReady)?
        {
            DisposalStage::Flushing(ticket) => ticket,
            DisposalStage::AwaitingWidgetRelease(_) => {
                return Ok(MainWindowComposerDisposalAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ));
            }
        };
        match self.advance_slot_flush(store, ticket)? {
            ComposerHostFlushAdvance::Progress(state) => {
                Ok(MainWindowComposerDisposalAdvance::Progress(state))
            }
            ComposerHostFlushAdvance::ReconciliationPending => {
                Ok(MainWindowComposerDisposalAdvance::ReconciliationPending)
            }
            ComposerHostFlushAdvance::Satisfied(
                ComposerHostFlushPurpose::Release | ComposerHostFlushPurpose::WindowClose,
            ) => {
                self.disposal_stage = Some(DisposalStage::AwaitingWidgetRelease(ticket));
                Ok(MainWindowComposerDisposalAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ))
            }
            ComposerHostFlushAdvance::Unsatisfied(_) => {
                Ok(MainWindowComposerDisposalAdvance::Failed)
            }
            ComposerHostFlushAdvance::Stale | ComposerHostFlushAdvance::Satisfied(_) => {
                Err(MainWindowComposerSlotError::StaleActivationReceipt)
            }
        }
    }

    pub fn complete_disposal_after_widget_release(
        &mut self,
        store: &HomeStore,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<MainWindowComposerDisposalAdvance, MainWindowComposerSlotError> {
        if !matches!(
            self.disposal_stage,
            Some(DisposalStage::AwaitingWidgetRelease(_))
        ) || self.selected_identity() != Some(release.selection())
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        match self
            .selected
            .as_mut()
            .unwrap()
            .host
            .dispose_composer_service(store)?
        {
            ComposerHostServiceDisposalCompletion::Disposed => {
                self.selected = None;
                self.disposal_stage = None;
                self.window_close = None;
                self.disposed = true;
                Ok(MainWindowComposerDisposalAdvance::Disposed)
            }
            ComposerHostServiceDisposalCompletion::Pending => {
                Ok(MainWindowComposerDisposalAdvance::ReconciliationPending)
            }
        }
    }

    pub(super) fn install_retiring(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        claim: WindowClaimSelection,
        retirement_operation_id: DraftPieceOperationIdV1,
        host: SyndicComposerHost,
    ) {
        let binding = host
            .binding()
            .expect("retiring composer host retains its activation binding");
        let dispatcher = MainWindowComposerDispatcher::new(binding);
        self.pending = Some(PendingComposer {
            receipt,
            claim,
            retirement_operation_id,
            host,
            dispatcher,
            source_selector: None,
            stage: PendingStage::Retiring,
            abandonment: None,
            abandonment_outcome: None,
            retain_thread_cleanup: self.capture_thread_cleanup,
            abandonment_canonical_home: None,
        });
    }

    pub(super) fn drive_retirement(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let pending = self.pending.as_mut().unwrap();
        if matches!(pending.stage, PendingStage::Departed) {
            return Ok(MainWindowComposerRetirementAdvance::DepartedFreshBoundary);
        }
        if pending.abandonment.is_none() {
            let request = if pending.retain_thread_cleanup {
                pending
                    .host
                    .thread_creation_fresh_abandonment_request(pending.retirement_operation_id)
            } else {
                pending
                    .host
                    .fresh_abandonment_request(pending.retirement_operation_id)
            }
            .ok_or(MainWindowComposerSlotError::TargetNotFresh)?;
            match self
                .storage
                .prepare_abandon_fresh_draft_editor_candidate_session(store, request)
            {
                Ok(prepared) => {
                    pending.abandonment = Some(prepared);
                    pending.abandonment_canonical_home = Some(store.canonical_path().to_owned());
                }
                Err(_) => {
                    pending.stage = PendingStage::Retiring;
                    return Ok(MainWindowComposerRetirementAdvance::Pending);
                }
            }
        }
        let prepared = pending.abandonment.as_ref().unwrap().clone();
        if let Some(outcome) = pending.abandonment_outcome.as_mut() {
            if let crate::composer_host::RetainedComposerCommandOutcome::Indeterminate {
                handle,
                result,
                ..
            } = outcome
            {
                let settled = match result {
                    None => Some(store.reconcile(handle)),
                    Some(Err(_)) => Some(store.retry_reconciliation(handle)),
                    Some(Ok(_)) => None,
                };
                if let Some(settled) = settled {
                    *result = Some(settled);
                }
            }
            match outcome.known_commit() {
                Some(true) => {
                    let classification = outcome.committed_classification().unwrap();
                    return self.finish_retirement_classification(store, &prepared, classification);
                }
                Some(false) => {}
                None => {
                    pending.stage = PendingStage::Reconciliation;
                    return Ok(MainWindowComposerRetirementAdvance::Pending);
                }
            }
        }
        let mut command = match store.home_revision() {
            Ok(revision) => HomeCommand::new(revision),
            Err(_) => {
                pending.stage = PendingStage::Reconciliation;
                return Ok(MainWindowComposerRetirementAdvance::Pending);
            }
        };
        let revision = match self.storage.revision(store) {
            Ok(revision) => revision,
            Err(_) => {
                pending.stage = PendingStage::Reconciliation;
                return Ok(MainWindowComposerRetirementAdvance::Pending);
            }
        };
        if command
            .add(
                self.storage
                    .abandon_fresh_draft_editor_candidate_session(revision, prepared.clone()),
            )
            .is_err()
        {
            pending.stage = PendingStage::Reconciliation;
            return Ok(MainWindowComposerRetirementAdvance::Pending);
        }
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.abandonment_before_execute_fault.take() {
            fault(store, self.storage.clone());
        }
        pending.abandonment_outcome = Some(
            crate::composer_host::RetainedComposerCommandOutcome::new(store.execute(command)),
        );
        let retained = pending.abandonment_outcome.as_ref().unwrap();
        let Some(outcome) = retained
            .committed_classification()
            .or_else(|| retained.noncommitted_classification())
        else {
            pending.stage = PendingStage::Reconciliation;
            return Ok(MainWindowComposerRetirementAdvance::Pending);
        };
        self.finish_retirement_classification(store, &prepared, outcome)
    }

    fn finish_retirement_classification(
        &mut self,
        store: &HomeStore,
        prepared: &syndic_storage::PreparedDraftEditorCandidateSessionAbandonFreshV1,
        outcome: beryl_home_store::CommandOutcome,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        let reconciled = self
            .storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(store, prepared, outcome);
        let pending = self.pending.as_mut().unwrap();
        match reconciled {
            Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_))
            | Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_))
            | Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(_)) => {
                if pending.retain_thread_cleanup {
                    if !matches!(
                        reconciled,
                        Ok(
                            DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
                                | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_)
                        )
                    ) || self.completed_thread_successor.is_some()
                    {
                        pending.stage = PendingStage::Reconciliation;
                        return Ok(MainWindowComposerRetirementAdvance::Pending);
                    }
                    let retired = self.pending.take().unwrap();
                    match self.retain_completed_thread_successor(store, retired) {
                        Ok(completed) => self.completed_thread_successor = Some(completed),
                        Err(pending) => {
                            self.pending = Some(pending);
                            return Ok(MainWindowComposerRetirementAdvance::Pending);
                        }
                    }
                    self.submission_successor = None;
                    return Ok(MainWindowComposerRetirementAdvance::Retired);
                }
                let mut retired = self.pending.take().unwrap();
                let _ = retired.host.dispose_composer_service(store);
                self.submission_successor = None;
                Ok(MainWindowComposerRetirementAdvance::Retired)
            }
            Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_))
            | Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(_)) => {
                pending.stage = PendingStage::Departed;
                Ok(MainWindowComposerRetirementAdvance::DepartedFreshBoundary)
            }
            Err(_) => {
                pending.stage = PendingStage::Reconciliation;
                Ok(MainWindowComposerRetirementAdvance::Pending)
            }
        }
    }
}
