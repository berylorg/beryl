use super::*;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn begin_committed_claim_disposal(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
        operation: DraftPieceOperationIdV1,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if self.selected_identity() != Some(expected)
            || !same_selected_host(Some(expected), receipt.expected_prior)
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let saved = match self.pending.as_ref().unwrap().stage {
            PendingStage::SelectionSaved(saved) => saved,
            PendingStage::SelectionDisposing(_) | PendingStage::AwaitingWidgetRelease => {
                return Ok(());
            }
            _ => return Err(MainWindowComposerSlotError::TargetNotReady),
        };
        let selected = self.selected.as_mut().unwrap();
        selected.host.validate_selection_save(store, saved)?;
        let captured = selected.host.capture_flush_disposal(
            store,
            saved.flush(),
            operation,
            &CommandCancellation::new(),
        )?;
        if !matches!(
            captured,
            crate::composer_host::ComposerHostFlushCapture::State(
                ComposerHostFlushState::DisposalRequired
            )
        ) {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        self.pending.as_mut().unwrap().stage = PendingStage::SelectionDisposing(saved.flush());
        Ok(())
    }

    pub(in crate::main_window) fn advance_committed_claim_disposal(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let ticket = match self.pending.as_ref().unwrap().stage {
            PendingStage::SelectionDisposing(ticket) => ticket,
            PendingStage::AwaitingWidgetRelease => {
                return Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ));
            }
            _ => return Err(MainWindowComposerSlotError::TargetNotReady),
        };
        match self.advance_slot_flush(store, ticket)? {
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::ThreadSwitch) => {
                self.pending.as_mut().unwrap().stage = PendingStage::AwaitingWidgetRelease;
                Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ))
            }
            ComposerHostFlushAdvance::Progress(state) => {
                Ok(MainWindowComposerPublishAdvance::Progress(state))
            }
            ComposerHostFlushAdvance::ReconciliationPending => {
                Ok(MainWindowComposerPublishAdvance::ReconciliationPending)
            }
            _ => Err(MainWindowComposerSlotError::TargetNotReady),
        }
    }
    pub(in crate::main_window) fn begin_claim_publication_save(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<ComposerHostFlushAdmission, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if !matches!(self.pending.as_ref().unwrap().stage, PendingStage::Ready)
            || !same_selected_host(self.selected_identity(), receipt.expected_prior)
            || !self.pending_source_is_current(store, receipt)?
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let admission = self
            .selected
            .as_mut()
            .unwrap()
            .host
            .begin_flush(ComposerHostFlushPurpose::ThreadSwitch)?;
        let ticket = match admission {
            ComposerHostFlushAdmission::Started { ticket, .. }
            | ComposerHostFlushAdmission::Joined { ticket, .. } => ticket,
            ComposerHostFlushAdmission::Satisfied(_) => {
                return Err(MainWindowComposerSlotError::IdentityMismatch);
            }
        };
        self.pending.as_mut().unwrap().stage = PendingStage::SelectionSaving(ticket);
        Ok(admission)
    }

    pub(in crate::main_window) fn advance_claim_selection_save(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let ticket = match self.pending.as_ref().unwrap().stage {
            PendingStage::SelectionSaving(ticket) => ticket,
            PendingStage::SelectionSaved(saved) => {
                self.selected
                    .as_ref()
                    .unwrap()
                    .host
                    .validate_selection_save(store, saved)?;
                return Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ));
            }
            _ => return self.advance_publish(store, receipt),
        };
        if !self.pending_source_is_current(store, receipt)? {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        match self.advance_slot_flush(store, ticket)? {
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired) => {
                let saved = self
                    .selected
                    .as_mut()
                    .unwrap()
                    .host
                    .qualify_selection_save(store, ticket)?;
                self.pending.as_mut().unwrap().stage = PendingStage::SelectionSaved(saved);
                Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ))
            }
            ComposerHostFlushAdvance::Progress(state) => {
                Ok(MainWindowComposerPublishAdvance::Progress(state))
            }
            ComposerHostFlushAdvance::ReconciliationPending => {
                Ok(MainWindowComposerPublishAdvance::ReconciliationPending)
            }
            ComposerHostFlushAdvance::Unsatisfied(_) => {
                self.pending.as_mut().unwrap().stage = PendingStage::Retiring;
                Ok(MainWindowComposerPublishAdvance::PriorFlushFailed)
            }
            ComposerHostFlushAdvance::Stale | ComposerHostFlushAdvance::Satisfied(_) => {
                Err(MainWindowComposerSlotError::StaleActivationReceipt)
            }
        }
    }
}
