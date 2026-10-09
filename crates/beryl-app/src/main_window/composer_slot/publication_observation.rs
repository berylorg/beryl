use super::*;
use crate::composer_host::ComposerHostPublicationExecutionObservation;

impl MainWindowComposerSlot {
    pub(crate) fn test_observe_selected_disposal_execution(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        observation: crate::composer_host::ComposerHostDisposalExecutionObservation,
    ) -> Result<(), String> {
        self.ensure_live().map_err(|error| error.to_string())?;
        self.selected_mut(selected)
            .map_err(|error| error.to_string())?
            .host
            .test_observe_disposal_execution(observation)
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn test_after_selected_publication_execute(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        hook: Box<
            dyn FnOnce(&beryl_home_store::HomeStore, &beryl_home_store::CommandOutcome) + Send,
        >,
    ) -> Result<(), String> {
        self.ensure_live().map_err(|error| error.to_string())?;
        self.selected_mut(selected)
            .map_err(|error| error.to_string())?
            .host
            .test_after_publication_execute(hook)
    }
    pub(crate) fn test_observe_selected_publication_execution(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        observation: ComposerHostPublicationExecutionObservation,
    ) -> Result<(), String> {
        self.ensure_live().map_err(|error| error.to_string())?;
        self.selected_mut(selected)
            .map_err(|error| error.to_string())?
            .host
            .test_observe_publication_execution(observation)
    }
}
