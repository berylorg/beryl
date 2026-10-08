use super::*;

impl MainWindowComposerSlot {
    pub(crate) fn begin_thread_creation_activation(
        &mut self,
        store: &HomeStore,
        claim: beryl_state::WindowClaimSelection,
        request: crate::composer_host::ComposerHostActivationRequest,
        retirement_operation_id: DraftPieceOperationIdV1,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<MainWindowComposerActivationAdvance, MainWindowComposerSlotError> {
        if self.capture_thread_cleanup || self.completed_thread_successor.is_some() {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        self.capture_thread_cleanup = true;
        let result =
            self.begin_activation(store, claim, request, retirement_operation_id, cancellation);
        self.capture_thread_cleanup = false;
        result
    }

    pub(crate) fn take_completed_thread_successor_cleanup(
        &mut self,
        target: beryl_state::WindowClaimSelection,
    ) -> Result<Option<MainWindowCompletedThreadSuccessorCleanup>, String> {
        if self
            .completed_thread_successor
            .as_ref()
            .is_some_and(|completed| completed.retired.selection.claim() != target)
        {
            return Err("completed successor cleanup has another original target".into());
        }
        Ok(self.completed_thread_successor.take())
    }

    pub(in crate::main_window::composer_slot) fn retain_completed_thread_successor(
        &self,
        store: &HomeStore,
        pending: PendingComposer,
    ) -> Result<MainWindowCompletedThreadSuccessorCleanup, PendingComposer> {
        let PendingComposer {
            receipt,
            claim,
            retirement_operation_id,
            host,
            dispatcher,
            source_selector,
            stage,
            abandonment,
            abandonment_outcome,
            retain_thread_cleanup,
            abandonment_canonical_home,
        } = pending;
        if !dispatcher.is_drained() || abandonment.is_none() || abandonment_outcome.is_none() {
            return Err(PendingComposer {
                receipt,
                claim,
                retirement_operation_id,
                host,
                dispatcher,
                source_selector,
                stage,
                abandonment,
                abandonment_outcome,
                retain_thread_cleanup,
                abandonment_canonical_home,
            });
        }
        match Box::new(host).retain_completed_thread_successor(
            store,
            abandonment.unwrap(),
            abandonment_outcome.unwrap(),
        ) {
            Ok(host) => Ok(MainWindowCompletedThreadSuccessorCleanup {
                retired: RetiredThreadSuccessor {
                    receipt,
                    selection: MainWindowComposerSelectionIdentity {
                        window_id: self.window_id,
                        claim,
                        binding: host.binding(),
                    },
                    host: Box::new(host),
                },
            }),
            Err((host, abandonment, abandonment_outcome)) => Err(PendingComposer {
                receipt,
                claim,
                retirement_operation_id,
                host: *host,
                dispatcher,
                source_selector,
                stage,
                abandonment: Some(abandonment),
                abandonment_outcome: Some(abandonment_outcome),
                retain_thread_cleanup,
                abandonment_canonical_home,
            }),
        }
    }
}
