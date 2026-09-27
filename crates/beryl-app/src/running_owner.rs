use std::{cell::RefCell, rc::Rc};

use gpui::{App, Entity};

use crate::{
    app_services::ProcessServiceOwner,
    main_window::PublishedMainWindowRestoreSet,
    startup_owner::{OwnedStartupSurface, RunningExitCommands, StartedProcess},
    theme_runtime::GpuiAppearanceWindowSet,
};

mod admission;
mod confirmation;
mod exit_attempt;
mod exit_availability;
mod exit_confirmation;
mod exit_delivery;
mod exit_draft_preparation;
mod exit_draft_recovery;
mod exit_notice;
mod exit_observation;
mod exit_placement_preparation;
mod exit_progress;
mod exit_refresh;
mod exit_routing;
mod exit_work;
mod initial_observation;
mod observation;
mod progress;
mod shutdown_drafts;
mod shutdown_interaction;
mod shutdown_placements;
mod shutdown_session;
pub(crate) use admission::{IdleShutdownError, RunningShutdownStatus};
pub(crate) use confirmation::{
    ShutdownConfirmationContext, ShutdownConfirmationResult, ShutdownIntent,
};
pub(crate) use exit_attempt::{ExitAttemptCompletion, ExitAttemptError};
pub(crate) use exit_confirmation::{ExitConfirmationError, ExitConfirmationRoute};
pub(crate) use exit_draft_preparation::ExitDraftPreparationCompletion;
pub(crate) use exit_observation::ExitObservationError;
pub(crate) use exit_placement_preparation::ExitPlacementPreparationCompletion;
pub(crate) use exit_progress::ExitProgressError;
pub(crate) use exit_routing::{ExitRoutingCompletion, ExitRoutingError};
pub(crate) use exit_work::{ExitWorkClassification, ExitWorkError, ExitWorkRoute};
pub(crate) use observation::ConfirmedShutdownAdmission;
pub(crate) use shutdown_drafts::{RunningShutdownDraftAction, RunningShutdownDraftProgress};
pub(crate) use shutdown_session::RunningShutdownSession;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StartupCleanup {
    Pending,
    Settled,
    Failed(String),
}

pub(crate) struct RunningProcessOwner {
    process: RunningProcess,
    startup_cleanup: StartupCleanup,
    confirmation: Option<confirmation::RunningConfirmation>,
    shutdown: Option<admission::RunningShutdownAttempt>,
    progress: Option<progress::RunningShutdownProgress>,
    observing_initial_work: bool,
    waiting_for_exit: bool,
    exit_availability: Option<gpui::Task<()>>,
    #[cfg(test)]
    exit_waiting_passes: usize,
    #[cfg(test)]
    confirmed_refreshes: usize,
}

pub(crate) struct RunningProcess {
    services: Option<ProcessServiceOwner>,
    pub(crate) windows: PublishedMainWindowRestoreSet,
    appearance: Entity<GpuiAppearanceWindowSet>,
    pub(crate) startup_surface: Option<OwnedStartupSurface>,
    commands: RunningExitCommands,
}

impl RunningProcessOwner {
    pub(crate) fn start(mut process: StartedProcess, app: &mut App) -> Rc<RefCell<Self>> {
        process.commands.bind_home(
            process
                .services
                .graph()
                .expect("running service graph")
                .home()
                .service_reference(),
        );
        let surface = process.startup_surface.take();
        let owner = Rc::new(RefCell::new(Self {
            process: RunningProcess {
                services: Some(process.services),
                windows: process.windows,
                appearance: process.appearance,
                startup_surface: None,
                commands: process.commands,
            },
            confirmation: None,
            shutdown: None,
            progress: None,
            observing_initial_work: false,
            waiting_for_exit: false,
            exit_availability: None,
            #[cfg(test)]
            exit_waiting_passes: 0,
            #[cfg(test)]
            confirmed_refreshes: 0,
            startup_cleanup: if surface.is_some() {
                StartupCleanup::Pending
            } else {
                StartupCleanup::Settled
            },
        }));
        Self::observe_exit_availability(&owner, app);
        if let Some(mut surface) = surface {
            let retained = owner.clone();
            app.spawn(async move |cx| {
                let result = surface.close(cx).await;
                let mut owner = retained.borrow_mut();
                owner.startup_cleanup = match result {
                    Ok(()) => StartupCleanup::Settled,
                    Err(error) => {
                        owner.process.startup_surface = Some(surface);
                        StartupCleanup::Failed(error)
                    }
                };
            })
            .detach();
        }
        owner
    }

    pub(crate) fn startup_cleanup(&self) -> &StartupCleanup {
        &self.startup_cleanup
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.process.commands.exit_requested()
    }

    #[cfg(test)]
    pub(crate) fn test_process(&self) -> &RunningProcess {
        &self.process
    }

    #[cfg(test)]
    pub(crate) fn test_services(&self) -> &ProcessServiceOwner {
        self.process
            .services
            .as_ref()
            .expect("services retained on GUI")
    }

    #[cfg(test)]
    pub(crate) fn test_into_process(self) -> StartedProcess {
        assert_ne!(self.startup_cleanup, StartupCleanup::Pending);
        assert!(self.progress.is_none());
        assert!(!self.observing_initial_work);
        assert!(!self.waiting_for_exit);
        StartedProcess {
            services: self.process.services.expect("services retained on GUI"),
            windows: self.process.windows,
            appearance: self.process.appearance,
            startup_surface: self.process.startup_surface,
            commands: self.process.commands,
        }
    }
}
