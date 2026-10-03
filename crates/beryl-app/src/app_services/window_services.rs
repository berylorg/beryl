use std::sync::Weak;

use super::*;
use crate::{
    main_window::{
        MainWindowCreationActivationSource, MainWindowCreationConfiguratorSource,
        MainWindowCreationRequestSource, MainWindowCreationServices, MainWindowRestoreSet,
        RestoredWindowActivationSource, RestoredWindowPreparationAttempt,
    },
    theme_runtime::AppearanceGeneration,
    window_acquisition::RuntimeBackedWindowAcquisitionService,
};
use beryl_model::{WindowId, WindowPlacement};

#[derive(Clone)]
pub(crate) struct MainWindowServiceInputs {
    pub(crate) request_source: MainWindowCreationRequestSource,
    pub(crate) activation_source: MainWindowCreationActivationSource,
    pub(crate) restored_activation_source: RestoredWindowActivationSource,
    pub(crate) configurator_source: MainWindowCreationConfiguratorSource,
}

pub(crate) struct PublishedMainWindowServices {
    creation: Arc<MainWindowCreationServices>,
    lifetime: Weak<()>,
    restored_activation: RestoredWindowActivationSource,
    exact_stop: PublishedExactStopWorker,
}

#[derive(Clone)]
pub(crate) struct PublishedExactStopWorker {
    worker: crate::cas_projection::ExactStopWorker,
    lifetime: Weak<()>,
    session: Option<beryl_state::SessionState>,
}

impl PublishedExactStopWorker {
    #[cfg(feature = "test-faults")]
    pub(crate) fn for_test(
        worker: crate::cas_projection::ExactStopWorker,
        lifetime: Weak<()>,
        session: beryl_state::SessionState,
    ) -> Self {
        Self {
            worker,
            lifetime,
            session: Some(session),
        }
    }
    pub(crate) fn selected_operation_snapshot(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> crate::cas_projection::ExactSelectedOperationSnapshot {
        if !self.selection_current(selection) {
            return crate::cas_projection::ExactSelectedOperationSnapshot::unavailable();
        }
        let snapshot = self
            .worker
            .selected_operation_snapshot(selection.claim().thread_id());
        if self.selection_current(selection) {
            snapshot
        } else {
            crate::cas_projection::ExactSelectedOperationSnapshot::unavailable()
        }
    }

    fn selection_current(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> bool {
        let identity = self.worker_identity();
        self.publication_current()
            && selection.binding().home_id() == identity.0
            && selection.binding().home_generation() == identity.1
            && self.session.as_ref().is_some_and(|session| {
                self.worker.window_selection_current(
                    session,
                    selection.window_id(),
                    selection.claim(),
                )
            })
    }

    pub(crate) fn request_selected_soft_stop(
        &self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        eligibility: &crate::cas_projection::ExactSoftStopEligibility,
    ) -> Result<
        crate::cas_projection::ExactStopFeedback,
        crate::cas_projection::ExactStopRequestError,
    > {
        if !self.selection_current(selection) {
            return Err(crate::cas_projection::ExactStopRequestError::Revoked);
        }
        self.request_exact_soft_stop(eligibility)
    }
    pub(crate) fn publication_current(&self) -> bool {
        self.lifetime.upgrade().is_some()
    }

    pub(crate) fn worker_identity(
        &self,
    ) -> (
        beryl_model::BerylHomeId,
        beryl_home_store::HomeGeneration,
        crate::cas_projection::ProjectionServiceGeneration,
    ) {
        (
            self.worker.home_id(),
            self.worker.home_generation(),
            self.worker.service_generation(),
        )
    }

    pub(crate) fn exact_soft_stop_eligibility(
        &self,
        thread: beryl_model::SyndicThreadId,
    ) -> crate::cas_projection::ExactSoftStopAvailability {
        self.eligibility_then(thread, || {})
    }

    pub(super) fn eligibility_then(
        &self,
        thread: beryl_model::SyndicThreadId,
        after_read: impl FnOnce(),
    ) -> crate::cas_projection::ExactSoftStopAvailability {
        use crate::cas_projection::{ExactSoftStopAvailability, ExactSoftStopUnavailable};
        if self.lifetime.upgrade().is_none() {
            return ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            );
        }
        let result = self.worker.exact_soft_stop_eligibility(thread);
        after_read();
        if self.lifetime.upgrade().is_none() {
            return ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            );
        }
        result
    }

    pub(crate) fn request_exact_soft_stop(
        &self,
        eligibility: &crate::cas_projection::ExactSoftStopEligibility,
    ) -> Result<
        crate::cas_projection::ExactStopFeedback,
        crate::cas_projection::ExactStopRequestError,
    > {
        if self.lifetime.upgrade().is_none() {
            return Err(crate::cas_projection::ExactStopRequestError::Revoked);
        }
        self.worker.request_exact_soft_stop(eligibility)
    }
}

#[cfg(feature = "test-faults")]
pub(crate) fn request_published_exact_soft_stop_for_test(
    worker: crate::cas_projection::ExactStopWorker,
    lifetime: Weak<()>,
    eligibility: &crate::cas_projection::ExactSoftStopEligibility,
) -> Result<crate::cas_projection::ExactStopFeedback, crate::cas_projection::ExactStopRequestError>
{
    PublishedExactStopWorker {
        worker,
        lifetime,
        session: None,
    }
    .request_exact_soft_stop(eligibility)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CloseConfirmationPreparationError {
    #[error("the complete shutdown services are unavailable")]
    Unavailable,
    #[error("the service graph already owns a shutdown attempt")]
    AlreadyShuttingDown,
    #[error(transparent)]
    Runtime(#[from] crate::cas_projection::RuntimeWorkError),
    #[error("{0}")]
    Window(String),
}

impl ProcessServiceOwner {
    pub(crate) fn published_exact_stop_worker(&self) -> Option<PublishedExactStopWorker> {
        let graph = self.graph()?;
        Some(PublishedExactStopWorker {
            worker: graph.cas().exact_stop_worker(),
            lifetime: Arc::downgrade(graph.restore_lifetime.as_ref()?),
            session: Some(graph.state().session()),
        })
    }
    pub(crate) fn admit_ordinary_close(
        &self,
        members: &[WindowId],
    ) -> Result<crate::window_acquisition::WindowCloseLease, String> {
        self.windows
            .admit_close(
                self.windows
                    .snapshot_for_close(members)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
    }
    pub(crate) fn admit_close_confirmation(
        &self,
        snapshot: crate::window_acquisition::WindowCloseSnapshot,
        invoking: WindowId,
        require_final: bool,
    ) -> Result<crate::window_acquisition::WindowCloseLease, String> {
        let final_member = self.inspect_close_confirmation(&snapshot, invoking)?;
        if require_final && !final_member {
            return Err("the invoking main window is no longer final".into());
        }
        self.windows
            .admit_close(snapshot)
            .map_err(|error| format!("close admission unavailable: {error}"))
    }

    pub(crate) fn prepare_close_confirmation(
        &self,
        members: &[WindowId],
        invoking: WindowId,
        observation: &crate::cas_projection::ShutdownWorkObservation,
    ) -> Result<
        (crate::window_acquisition::WindowCloseSnapshot, bool),
        CloseConfirmationPreparationError,
    > {
        let graph = self
            .graph()
            .ok_or(CloseConfirmationPreparationError::Unavailable)?;
        if graph.shutdown.is_some() {
            return Err(CloseConfirmationPreparationError::AlreadyShuttingDown);
        }
        graph
            .cas
            .as_ref()
            .ok_or(CloseConfirmationPreparationError::Unavailable)?
            .try_validate_shutdown_runtime(&graph.sessions, observation.revision())?;
        let snapshot = self.windows.snapshot_for_close(members).map_err(|error| {
            CloseConfirmationPreparationError::Window(format!(
                "close snapshot unavailable: {error:?}"
            ))
        })?;
        let final_member = self
            .inspect_close_confirmation(&snapshot, invoking)
            .map_err(CloseConfirmationPreparationError::Window)?;
        Ok((snapshot, final_member))
    }

    pub(crate) fn inspect_close_confirmation(
        &self,
        snapshot: &crate::window_acquisition::WindowCloseSnapshot,
        invoking: WindowId,
    ) -> Result<bool, String> {
        self.windows
            .inspect_close_snapshot(snapshot, invoking)
            .map_err(|error| format!("close snapshot unavailable: {error:?}"))
    }

    pub(crate) fn window_services(
        &self,
        inputs: MainWindowServiceInputs,
    ) -> Result<PublishedMainWindowServices, String> {
        let graph = self
            .graph()
            .ok_or_else(|| "the complete service graph is unavailable".to_owned())?;
        if graph.shutdown.is_some() {
            return Err("service graph is shutting down".to_owned());
        }
        self.process
            .admit(|| ())
            .map_err(|error| error.to_string())?;
        let lifetime = Arc::downgrade(
            graph
                .restore_lifetime
                .as_ref()
                .ok_or_else(|| "restore service generation is retired".to_owned())?,
        );
        let creation = self.build_creation_services(
            inputs.request_source,
            inputs.activation_source,
            inputs.configurator_source,
        )?;
        creation.validate_source()?;
        Ok(PublishedMainWindowServices {
            creation,
            exact_stop: PublishedExactStopWorker {
                worker: graph.cas().exact_stop_worker(),
                lifetime: lifetime.clone(),
                session: Some(graph.state().session()),
            },
            lifetime,
            restored_activation: inputs.restored_activation_source,
        })
    }

    pub(crate) fn recovered_creation_services(
        &self,
        previous: &MainWindowCreationServices,
        previous_generation: beryl_home_store::HomeGeneration,
    ) -> Result<Arc<MainWindowCreationServices>, String> {
        let graph = self
            .graph()
            .ok_or("recovered creation graph is unavailable")?;
        if graph.home().home_id() != previous.store.home_id()
            || graph.home().health().generation() == Some(previous_generation)
            || graph.home().health().state() != beryl_home_store::HomeHealthState::Healthy
        {
            return Err("creation replacement is not a fresh same-home graph".into());
        }
        self.build_creation_services(
            previous.request_source.clone(),
            previous.activation_source.clone(),
            previous.configurator_source.clone(),
        )
    }

    fn build_creation_services(
        &self,
        request_source: MainWindowCreationRequestSource,
        activation_source: MainWindowCreationActivationSource,
        configurator_source: MainWindowCreationConfiguratorSource,
    ) -> Result<Arc<MainWindowCreationServices>, String> {
        let graph = self
            .graph()
            .ok_or("creation service graph is unavailable")?;
        let store = Arc::new(graph.home().service_reference());
        let creation = Arc::new(MainWindowCreationServices {
            acquisition: RuntimeBackedWindowAcquisitionService::new(
                &self.windows,
                store.clone(),
                graph.state.clone(),
                graph.syndic.clone(),
            ),
            store,
            state: graph.state.clone(),
            storage: graph.syndic.clone(),
            request_source,
            activation_source,
            configurator_source,
            marker_seals: graph.marker(),
            turn_start_requirement: graph.cas().config().turn_start_admission_requirement(),
            submission_execution: graph.cas().submission_execution_wake(),
            #[cfg(feature = "test-faults")]
            test_before_initial_advance: None,
        });
        creation.validate_prepared_source()?;
        Ok(creation)
    }
}

impl PublishedMainWindowServices {
    pub(crate) fn exact_stop_worker(&self) -> PublishedExactStopWorker {
        self.exact_stop.clone()
    }
    pub(crate) fn creation_services(&self) -> Arc<MainWindowCreationServices> {
        self.creation.clone()
    }

    pub(crate) fn into_restore_set(
        self,
        appearance: Arc<AppearanceGeneration>,
        initial_window: WindowId,
        initial_placement: WindowPlacement,
    ) -> Result<MainWindowRestoreSet, String> {
        self.validate()?;
        let attempt = RestoredWindowPreparationAttempt::new(
            self.creation.store.clone(),
            self.creation.state.session(),
            self.creation.storage.clone(),
            self.lifetime.clone(),
        )?;
        self.validate()?;
        let restore = MainWindowRestoreSet::new(
            self.creation.clone(),
            attempt,
            self.restored_activation.clone(),
            appearance,
            initial_window,
            initial_placement,
        )?;
        self.validate()?;
        Ok(restore)
    }

    fn validate(&self) -> Result<(), String> {
        if self.lifetime.upgrade().is_none() {
            return Err("restore service generation is retired".to_owned());
        }
        self.creation.validate_source()
    }
}
