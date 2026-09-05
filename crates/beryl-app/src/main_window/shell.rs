use std::{cell::RefCell, rc::Rc, sync::Arc};

use beryl_home_store::CommandCancellation;
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, Styled, Window, WindowHandle, WindowOptions, div, px,
};

use super::{
    MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerMount,
    MainWindowConversationComposerPreparedSelection, MainWindowConversationComposerService,
};
use crate::composer_marker_seal::DraftMarkerSealService;
use crate::window_acquisition::{
    RuntimeBackedWindowAbandonment, RuntimeBackedWindowAbandonmentAuditSeed,
    RuntimeBackedWindowAbandonmentNotCommitted, RuntimeBackedWindowAbandonmentOutcome,
    RuntimeBackedWindowAbandonmentPreparationOutcome, RuntimeBackedWindowAbandonmentReconciliation,
    RuntimeBackedWindowAbandonmentReconciliationOutcome, RuntimeBackedWindowAcquisition,
    RuntimeBackedWindowAcquisitionService, RuntimeBackedWindowMainWindowReservation,
    RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
};

mod appearance;
use crate::theme_runtime::{AppearanceGeneration, GpuiAppearanceWindowSet};
use appearance::MainWindowShellAppearance;

pub type MainWindowShellComposerConfigurator = Box<
    dyn FnMut(
            super::MainWindowComposerSelectionIdentity,
        ) -> Result<super::MainWindowConversationComposerConfig, String>
        + Send,
>;

pub struct MainWindowShellPreparationRequest {
    acquisition: RuntimeBackedWindowAcquisition,
    composer_service: Arc<MainWindowConversationComposerService>,
    composer_configurator: MainWindowShellComposerConfigurator,
    marker_seals: DraftMarkerSealService,
    submission_request_source: MainWindowComposerSubmissionRequestSource,
    appearance: Arc<AppearanceGeneration>,
}

impl MainWindowShellPreparationRequest {
    #[must_use]
    pub fn new(
        acquisition: RuntimeBackedWindowAcquisition,
        composer_service: Arc<MainWindowConversationComposerService>,
        composer_configurator: MainWindowShellComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        appearance: Arc<AppearanceGeneration>,
    ) -> Self {
        Self {
            acquisition,
            composer_service,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        }
    }
}

pub enum MainWindowShellPreparationFailure {
    Reservation {
        acquisition: RuntimeBackedWindowAcquisition,
        error: RuntimeBackedWindowMainWindowReservationError,
    },
    Composer {
        error: String,
        unpublished: MainWindowShellUnpublished,
    },
}

pub struct MainWindowShellPrepared {
    acquisition: RuntimeBackedWindowAcquisition,
    reservation: RuntimeBackedWindowMainWindowReservation,
    composer: MainWindowConversationComposerPreparedSelection,
    composer_configurator: MainWindowShellComposerConfigurator,
    marker_seals: DraftMarkerSealService,
    submission_request_source: MainWindowComposerSubmissionRequestSource,
    appearance: MainWindowShellAppearance,
}

impl MainWindowShellPrepared {
    pub fn prepare(
        registry: &RuntimeBackedWindowProcessRegistry,
        acquisition_service: &RuntimeBackedWindowAcquisitionService,
        request: MainWindowShellPreparationRequest,
    ) -> Result<Self, MainWindowShellPreparationFailure> {
        let MainWindowShellPreparationRequest {
            acquisition,
            composer_service,
            mut composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        } = request;
        let reservation = match registry.reserve_main_window(acquisition.window_id()) {
            Ok(reservation) => reservation,
            Err(error) => {
                return Err(MainWindowShellPreparationFailure::Reservation { error, acquisition });
            }
        };
        let composer = match MainWindowConversationComposerMount::prepare_selected(
            composer_service,
            &mut composer_configurator,
        ) {
            Ok(composer) => composer,
            Err(error) => {
                return Err(MainWindowShellPreparationFailure::Composer {
                    error,
                    unpublished: MainWindowShellUnpublished {
                        acquisition,
                        reservation,
                    },
                });
            }
        };
        let selection = composer.selection_identity();
        let binding = selection.binding();
        let appearance_home = appearance.prepared().home();
        let validation = acquisition_service
            .validate_shell_selection(&acquisition, selection)
            .and_then(|()| {
                if appearance_home.home_id() == binding.home_id()
                    && appearance_home.home_generation() == binding.home_generation()
                {
                    Ok(())
                } else {
                    Err("shell appearance belongs to a different home generation".to_owned())
                }
            });
        if let Err(error) = validation {
            return Err(MainWindowShellPreparationFailure::Composer {
                error,
                unpublished: MainWindowShellUnpublished {
                    acquisition,
                    reservation,
                },
            });
        }
        Ok(Self {
            acquisition,
            reservation,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance: MainWindowShellAppearance::prepare(appearance),
        })
    }

    #[must_use]
    pub const fn window_id(&self) -> beryl_model::WindowId {
        self.acquisition.window_id()
    }

    #[must_use]
    pub fn appearance(&self) -> &Arc<AppearanceGeneration> {
        &self.appearance.generation
    }

    #[must_use]
    pub fn into_unpublished(self) -> MainWindowShellUnpublished {
        MainWindowShellUnpublished {
            acquisition: self.acquisition,
            reservation: self.reservation,
        }
    }
}

pub trait MainWindowShellHost {
    type Shell;
    type Error;

    fn construct_hidden(
        &mut self,
        prepared: MainWindowShellPrepared,
    ) -> Result<Self::Shell, MainWindowShellHostFailure<Self::Error>>;
}

pub enum MainWindowShellHostFailure<E> {
    BeforeConstruction {
        error: E,
        prepared: MainWindowShellPrepared,
    },
    Construction {
        error: E,
        unpublished: MainWindowShellUnpublished,
    },
}

impl<E> MainWindowShellHostFailure<E> {
    #[must_use]
    pub fn into_unpublished(self) -> MainWindowShellUnpublished {
        match self {
            Self::BeforeConstruction { prepared, .. } => prepared.into_unpublished(),
            Self::Construction { unpublished, .. } => unpublished,
        }
    }
}

mod custody;
pub use custody::*;
mod host;
pub use host::*;
