use super::*;

impl MainWindowConversationComposerService {
    #[cfg(test)]
    pub(crate) fn test_observe_selected_disposal_execution(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        observation: crate::composer_host::ComposerHostDisposalExecutionObservation,
    ) -> Result<(), String> {
        self.ensure_no_window_close()?;
        self.slot
            .try_lock()
            .map_err(|_| "original disposal observation slot is busy")?
            .test_observe_selected_disposal_execution(selected, observation)
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_after_selected_publication_execute(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        hook: Box<
            dyn FnOnce(&beryl_home_store::HomeStore, &beryl_home_store::CommandOutcome) + Send,
        >,
    ) -> Result<(), String> {
        self.ensure_no_window_close()?;
        self.slot
            .try_lock()
            .map_err(|_| "original publication outcome slot is busy")?
            .test_after_selected_publication_execute(selected, hook)
    }
    #[cfg(test)]
    pub(crate) fn test_observe_selected_publication_execution(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        observation: crate::composer_host::ComposerHostPublicationExecutionObservation,
    ) -> Result<(), String> {
        self.ensure_no_window_close()?;
        self.slot
            .try_lock()
            .map_err(|_| "original publication observation slot is busy")?
            .test_observe_selected_publication_execution(selected, observation)
    }

    pub(crate) fn capture_ordinary_claim_save(
        &self,
        receipt: crate::main_window::MainWindowComposerActivationReceipt,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowRetiredClaimPredecessorSave, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "ordinary selection save source lock failed")?
            .capture_ordinary_claim_save(&self.store, receipt, selected)
    }

    pub(crate) fn begin_ordinary_claim_activation(
        &self,
        claim: beryl_state::WindowClaimSelection,
        request: crate::composer_host::ComposerHostActivationRequest,
        retirement: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<crate::main_window::MainWindowComposerActivationAdvance, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "ordinary activation source lock failed")?
            .begin_ordinary_claim_activation(&self.store, claim, request, retirement, cancellation)
            .map_err(|error| error.to_string())
    }
}
