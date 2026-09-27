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
}

impl ProcessServiceOwner {
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
    ) -> Result<(crate::window_acquisition::WindowCloseSnapshot, bool), String> {
        let graph = self
            .graph()
            .ok_or("the complete service graph is unavailable")?;
        if graph.shutdown.is_some() {
            return Err("the service graph already owns a shutdown attempt".into());
        }
        graph
            .cas
            .as_ref()
            .ok_or("the runtime service is unavailable")?
            .try_validate_shutdown_runtime(&graph.sessions, observation.revision())
            .map_err(|error| error.to_string())?;
        let snapshot = self
            .windows
            .snapshot_for_close(members)
            .map_err(|error| format!("close snapshot unavailable: {error:?}"))?;
        let final_member = self.inspect_close_confirmation(&snapshot, invoking)?;
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
            request_source: inputs.request_source,
            activation_source: inputs.activation_source,
            configurator_source: inputs.configurator_source,
            marker_seals: graph.marker(),
            turn_start_requirement: graph.cas().config().turn_start_admission_requirement(),
            submission_execution: graph.cas().submission_execution_wake(),
            #[cfg(feature = "test-faults")]
            test_before_initial_advance: None,
        });
        creation.validate_source()?;
        Ok(PublishedMainWindowServices {
            creation,
            lifetime,
            restored_activation: inputs.restored_activation_source,
        })
    }
}

impl PublishedMainWindowServices {
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
