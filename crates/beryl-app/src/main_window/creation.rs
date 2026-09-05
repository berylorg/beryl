pub(in crate::main_window) mod command;
mod owner;
mod work;
pub use owner::*;

use super::*;
use crate::composer_host::ComposerHostActivationRequest;
use crate::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::WindowId;
use beryl_state::{BerylState, RememberedTarget};
use std::sync::Arc;
use syndic_storage::{DraftPieceOperationIdV1, SyndicStorage};

pub type MainWindowCreationRequestSource = Arc<
    dyn Fn(WindowId, RememberedTarget) -> Result<RuntimeBackedWindowAcquisitionRequest, String>
        + Send
        + Sync,
>;
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
    pub store: Arc<HomeStore>,
    pub state: BerylState,
    pub storage: SyndicStorage,
    pub request_source: MainWindowCreationRequestSource,
    pub activation_source: MainWindowCreationActivationSource,
    pub configurator_source: MainWindowCreationConfiguratorSource,
    pub marker_seals: crate::composer_marker_seal::DraftMarkerSealService,
    pub turn_start_requirement: beryl_home_store::TurnStartAdmissionRequirement,
    #[cfg(feature = "test-faults")]
    pub test_before_initial_advance:
        Option<Arc<dyn Fn(&mut MainWindowInitialComposer) + Send + Sync>>,
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
    ) -> Result<Self, RuntimeBackedWindowMainWindowReservationError> {
        let reservation = services
            .acquisition
            .process_registry()
            .reserve_main_window(window_id)?;
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
