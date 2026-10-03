use super::*;

impl PublishedExactStopWorker {
    #[cfg(feature = "test-faults")]
    pub(crate) fn for_test_with_retry(
        worker: crate::cas_projection::ExactStopWorker,
        runtime_retry: crate::cas_projection::SelectedRuntimeRetryWorker,
        lifetime: Weak<()>,
        session: beryl_state::SessionState,
    ) -> Self {
        Self {
            worker,
            runtime_retry: Some(runtime_retry),
            lifetime,
            session: Some(session),
        }
    }

    pub(crate) fn selected_runtime_retry_eligible(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        execution: &beryl_model::ExecutionBinding,
        failure: crate::cas_projection::RuntimeFailureSnapshot,
    ) -> bool {
        let Some(worker) = self.runtime_retry.as_ref() else {
            return false;
        };
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        self.selection_current(selection)
            && worker.eligible(
                session,
                selection.window_id(),
                selection.claim(),
                execution,
                failure,
            )
            && self.publication_current()
    }

    pub(crate) fn retry_selected_runtime(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        execution: &beryl_model::ExecutionBinding,
        failure: crate::cas_projection::RuntimeFailureSnapshot,
        cancellation: &crate::cas_projection::ProjectionCancellationToken,
    ) -> Result<
        crate::cas_projection::SelectedRuntimeUsability,
        crate::cas_projection::SelectedRuntimeRetryError,
    > {
        use crate::cas_projection::SelectedRuntimeRetryError;
        if !self.selection_current(selection) {
            return Err(SelectedRuntimeRetryError::Revoked);
        }
        let worker = self
            .runtime_retry
            .as_ref()
            .ok_or(SelectedRuntimeRetryError::Unavailable)?;
        let session = self
            .session
            .as_ref()
            .ok_or(SelectedRuntimeRetryError::Revoked)?;
        let proof = worker.recover(
            session,
            selection.window_id(),
            selection.claim(),
            execution,
            failure,
            cancellation,
        )?;
        if !self.publication_current() {
            return Err(SelectedRuntimeRetryError::Revoked);
        }
        Ok(proof)
    }

    pub(crate) fn selected_runtime_usability_current(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        proof: &crate::cas_projection::SelectedRuntimeUsability,
    ) -> bool {
        let Some(worker) = self.runtime_retry.as_ref() else {
            return false;
        };
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        self.selection_current(selection)
            && proof.matches_selection(selection.window_id(), selection.claim())
            && worker.usability_current(session, proof)
            && self.publication_current()
    }

    pub(crate) fn with_selected_runtime_usability_publication<T>(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        proof: &crate::cas_projection::SelectedRuntimeUsability,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        let _lifetime = self.lifetime.upgrade()?;
        let identity = self.worker_identity();
        if selection.binding().home_id() != identity.0
            || selection.binding().home_generation() != identity.1
            || !proof.matches_selection(selection.window_id(), selection.claim())
        {
            return None;
        }
        self.runtime_retry
            .as_ref()?
            .with_usability_publication(proof, publish)
    }
}
