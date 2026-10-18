use super::*;
use crate::cas_projection::{ProjectionCancellationToken, RuntimeInterest, RuntimeInterestError};
use crate::main_window::MainWindowComposerSelectionIdentity;

impl PublishedExactStopWorker {
    pub(crate) fn acquire_selected_view_interest(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(RuntimeInterest, beryl_home_store::HomeMutationObservation), RuntimeInterestError>
    {
        if cancellation.is_cancelled() || !self.selection_current(selection) {
            return Err(RuntimeInterestError::Closed);
        }
        let interest = self
            .runtime_retry
            .as_ref()
            .ok_or(RuntimeInterestError::NotConfigured)?
            .acquire_selected_view_interest(
                self.session.as_ref().ok_or(RuntimeInterestError::Closed)?,
                selection.window_id(),
                selection.claim(),
                cancellation,
            )?;
        if cancellation.is_cancelled() || !self.selection_current(selection) {
            return Err(RuntimeInterestError::Closed);
        }
        Ok(interest)
    }

    pub(crate) fn selected_view_binding_observation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        binding: &beryl_model::ExecutionBinding,
    ) -> Result<(bool, beryl_home_store::HomeMutationObservation), RuntimeInterestError> {
        if !self.publication_current() {
            return Err(RuntimeInterestError::Closed);
        }
        self.runtime_retry
            .as_ref()
            .ok_or(RuntimeInterestError::NotConfigured)?
            .selected_view_binding_observation(
                self.session.as_ref().ok_or(RuntimeInterestError::Closed)?,
                selection.window_id(),
                selection.claim(),
                binding,
            )
    }

    pub(crate) fn elect_selected_view_observation<T>(
        &self,
        observation: &beryl_home_store::HomeMutationObservation,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        if !self.publication_current() {
            return None;
        }
        self.runtime_retry
            .as_ref()?
            .elect_selected_view_observation(observation, || {
                self.publication_current().then(publish)
            })
            .flatten()
    }
}
