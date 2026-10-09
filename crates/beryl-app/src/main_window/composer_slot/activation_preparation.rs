use super::*;

impl MainWindowComposerSlot {
    #[inline(never)]
    pub(super) fn prepare_activation_host(storage: SyndicStorage) -> Box<SyndicComposerHost> {
        Box::new(SyndicComposerHost::new(storage))
    }

    #[inline(never)]
    pub(super) fn install_prepared_activation(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        claim: WindowClaimSelection,
        retirement_operation_id: DraftPieceOperationIdV1,
        host: Box<SyndicComposerHost>,
        source_selector: DraftEditorCurrentSelectorV1,
    ) -> Result<(), MainWindowComposerSlotError> {
        let binding = host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        let dispatcher = MainWindowComposerDispatcher::new(binding);
        self.pending = Some(PendingComposer {
            receipt,
            claim,
            retirement_operation_id,
            host: *host,
            dispatcher,
            source_selector: Some(source_selector),
            stage: PendingStage::Ready,
            abandonment: None,
            abandonment_outcome: None,
            retain_thread_cleanup: self.capture_thread_cleanup,
            abandonment_canonical_home: None,
        });
        Ok(())
    }

    #[inline(never)]
    pub(super) fn install_prepared_retirement(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        claim: WindowClaimSelection,
        retirement_operation_id: DraftPieceOperationIdV1,
        host: Box<SyndicComposerHost>,
    ) {
        self.install_retiring(receipt, claim, retirement_operation_id, *host);
    }
}
