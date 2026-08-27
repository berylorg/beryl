use beryl_home_store::HomeCommand;
use syndic_storage::DraftEditorCandidateSessionAbandonFreshOutcomeV1;

use super::*;
use crate::composer_host::ComposerHostServiceDisposalCompletion;

impl MainWindowComposerSlot {
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
        match pending.host.dispose_composer_service(store)? {
            ComposerHostServiceDisposalCompletion::Pending => {
                Ok(MainWindowComposerRetirementAdvance::Pending)
            }
            ComposerHostServiceDisposalCompletion::Disposed => {
                self.pending = None;
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
            PendingStage::Publishing(_) | PendingStage::AwaitingWidgetRelease
        ) {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        self.pending.as_mut().unwrap().stage = PendingStage::Retiring;
        self.drive_retirement(store, receipt)
    }

    pub fn reconcile_pending_after_recovery(
        &mut self,
        store: &HomeStore,
        recovered_storage: SyndicStorage,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerRetirementAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let pending = self.pending.as_ref().unwrap();
        if !matches!(pending.stage, PendingStage::Reconciliation) || pending.abandonment.is_none() {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        recovered_storage
            .revision(store)
            .map_err(|_| MainWindowComposerSlotError::RecoveryHandleMismatch)?;
        self.storage = recovered_storage;
        self.drive_retirement(store, receipt)
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
            ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::Release) => {
                self.disposal_stage = Some(DisposalStage::AwaitingWidgetRelease)
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
            DisposalStage::AwaitingWidgetRelease => {
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
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release) => {
                self.disposal_stage = Some(DisposalStage::AwaitingWidgetRelease);
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
            Some(DisposalStage::AwaitingWidgetRelease)
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
        let dispatcher = MainWindowComposerDispatcher::new(binding, &host);
        self.pending = Some(PendingComposer {
            receipt,
            claim,
            retirement_operation_id,
            host,
            dispatcher,
            source_selector: None,
            stage: PendingStage::Retiring,
            abandonment: None,
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
            let request = pending
                .host
                .fresh_abandonment_request(pending.retirement_operation_id)
                .ok_or(MainWindowComposerSlotError::TargetNotFresh)?;
            match self
                .storage
                .prepare_abandon_fresh_draft_editor_candidate_session(store, request)
            {
                Ok(prepared) => pending.abandonment = Some(prepared),
                Err(_) => {
                    pending.stage = PendingStage::Retiring;
                    return Ok(MainWindowComposerRetirementAdvance::Pending);
                }
            }
        }
        let prepared = pending.abandonment.as_ref().unwrap().clone();
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
            fault(store, self.storage);
        }
        let outcome = store.execute(command);
        let reconciled = self
            .storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(store, &prepared, outcome);
        match reconciled {
            Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_))
            | Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_))
            | Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(_)) => {
                let mut retired = self.pending.take().unwrap();
                let _ = retired.host.dispose_composer_service(store);
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
