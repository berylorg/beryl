pub(in crate::main_window) mod command;
mod owner;
mod work;
pub use owner::*;

use super::*;
use crate::composer_host::ComposerHostActivationRequest;
use crate::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeServiceReference};
use beryl_model::WindowId;
use beryl_state::{BerylState, RememberedTarget};
use std::sync::Arc;
use syndic_storage::{DraftPieceOperationIdV1, SyndicStorage};

pub type MainWindowCreationRequestSource = Arc<
    dyn Fn(
            WindowId,
            RememberedTarget,
            MainWindowCreationRequestContext<'_>,
        ) -> Result<RuntimeBackedWindowAcquisitionRequest, String>
        + Send
        + Sync,
>;

pub struct MainWindowCreationRequestContext<'a> {
    pub(in crate::main_window) store: &'a HomeServiceReference,
    pub(in crate::main_window) state: &'a BerylState,
}

impl MainWindowCreationRequestContext<'_> {
    pub(crate) fn new<'a>(
        store: &'a HomeServiceReference,
        state: &'a BerylState,
    ) -> MainWindowCreationRequestContext<'a> {
        MainWindowCreationRequestContext { store, state }
    }
    pub fn execution_binding(
        &self,
        target: RememberedTarget,
    ) -> Result<beryl_model::ExecutionBinding, String> {
        let before = self.store.home_revision().map_err(|e| e.to_string())?;
        let generation = self
            .store
            .health()
            .generation()
            .ok_or("window creation home is unavailable")?;
        let runtime = self
            .state
            .runtime_roots()
            .runtime(self.store, target.runtime_id())
            .map_err(|e| e.to_string())?
            .ok_or("remembered runtime is unavailable")?;
        let root = self
            .state
            .runtime_roots()
            .root(self.store, target.root_id())
            .map_err(|e| e.to_string())?
            .ok_or("remembered root is unavailable")?;
        if root.runtime_id() != runtime.runtime_id()
            || root.canonical_path().mode() != runtime.mode()
        {
            return Err("remembered runtime and root no longer match".into());
        }
        if self.store.health().state() != beryl_home_store::HomeHealthState::Healthy
            || self.store.health().generation() != Some(generation)
            || self.store.home_revision().map_err(|e| e.to_string())? != before
        {
            return Err("window creation target changed during resolution".into());
        }
        Ok(beryl_model::ExecutionBinding::new(
            target.runtime_id(),
            target.root_id(),
            root.canonical_path().clone(),
        ))
    }
}
pub type MainWindowCreationActivationSource = Arc<
    dyn Fn(
            &RuntimeBackedWindowAcquisition,
        ) -> Result<(ComposerHostActivationRequest, DraftPieceOperationIdV1), String>
        + Send
        + Sync,
>;
pub type MainWindowCreationConfiguratorSource =
    Arc<dyn Fn() -> MainWindowShellComposerConfigurator + Send + Sync>;

pub struct MainWindowCreationServices {
    pub acquisition: RuntimeBackedWindowAcquisitionService,
    pub store: Arc<HomeServiceReference>,
    pub state: BerylState,
    pub storage: SyndicStorage,
    pub request_source: MainWindowCreationRequestSource,
    pub activation_source: MainWindowCreationActivationSource,
    pub configurator_source: MainWindowCreationConfiguratorSource,
    pub marker_seals: crate::composer_marker_seal::DraftMarkerSealService,
    pub turn_start_requirement: beryl_home_store::TurnStartAdmissionRequirement,
    pub submission_execution: crate::cas_projection::SubmissionExecutionWake,
    #[cfg(feature = "test-faults")]
    pub test_before_initial_advance:
        Option<Arc<dyn Fn(&mut MainWindowInitialComposer) + Send + Sync>>,
}

impl MainWindowCreationServices {
    pub(crate) fn validate_source(&self) -> Result<(), String> {
        self.validate_prepared_source()?;
        self.submission_execution
            .execution_candidate()
            .map_err(|error| {
                format!("window creation execution authority is unavailable: {error}")
            })?;
        Ok(())
    }

    pub(crate) fn validate_prepared_source(&self) -> Result<(), String> {
        if !Arc::ptr_eq(&self.store, &self.acquisition.home_reference()) {
            return Err("window creation services have different source custody".to_owned());
        }
        let health = self.store.health();
        if health.state() != beryl_home_store::HomeHealthState::Healthy {
            return Err("window creation home authority is unavailable".to_owned());
        }
        let generation = health
            .generation()
            .ok_or_else(|| "window creation home generation is unavailable".to_owned())?;
        if !self
            .submission_execution
            .matches_binding(self.store.home_id(), generation)
        {
            return Err(
                "window submission authority belongs to another home generation".to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum MainWindowCreationAdmissionError {
    Services(String),
    Reservation(RuntimeBackedWindowMainWindowReservationError),
}

pub struct MainWindowCreation {
    services: Arc<MainWindowCreationServices>,
    window_id: WindowId,
    cancellation: CommandCancellation,
    state: work::CreationState,
    error: Option<String>,
}

pub enum MainWindowCreationOutcome {
    Pending(MainWindowCreation),
    Prepared {
        prepared: MainWindowShellPrepared,
        cancellation: CommandCancellation,
    },
    Settled {
        window_id: WindowId,
        error: Option<String>,
    },
}

impl MainWindowCreation {
    pub fn admit(
        services: Arc<MainWindowCreationServices>,
        window_id: WindowId,
        target: RememberedTarget,
    ) -> Result<Self, MainWindowCreationAdmissionError> {
        services
            .validate_source()
            .map_err(MainWindowCreationAdmissionError::Services)?;
        let reservation = services
            .acquisition
            .process_registry()
            .reserve_main_window(window_id)
            .map_err(MainWindowCreationAdmissionError::Reservation)?;
        Ok(Self {
            services,
            window_id,
            cancellation: CommandCancellation::new(),
            state: work::CreationState::Request {
                target,
                reservation,
            },
            error: None,
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub fn cancellation(&self) -> CommandCancellation {
        self.cancellation.clone()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn abandon(
        services: Arc<MainWindowCreationServices>,
        unpublished: MainWindowShellUnpublished,
        error: String,
    ) -> Self {
        Self {
            window_id: unpublished.window_id(),
            services,
            cancellation: CommandCancellation::new(),
            state: work::CreationState::Unpublished(unpublished),
            error: Some(error),
        }
    }
}
