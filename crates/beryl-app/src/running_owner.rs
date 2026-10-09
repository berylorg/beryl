use std::{cell::RefCell, rc::Rc};

use gpui::{App, Entity};

use crate::{
    app_services::{AppServiceConfiguration, ProcessServiceOwner},
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
mod exit_session_publication;
mod exit_work;
mod final_teardown;
mod initial_observation;
mod observation;
mod ordinary_close_session;
mod ordinary_commands;
mod progress;
mod running_threads_attention;
mod running_threads_commands;
mod runtime_setup;
mod shutdown_drafts;
mod unchanged_running;
mod unremoved_windows;
#[cfg(test)]
pub(crate) use shutdown_drafts::RunningShutdownDrafts;
#[cfg(test)]
pub(crate) use unremoved_windows::UnremovedWindows;
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
pub(crate) use final_teardown::RunningExitCompletion;
pub(crate) use observation::ConfirmedShutdownAdmission;
pub(crate) use shutdown_drafts::{RunningShutdownDraftAction, RunningShutdownDraftProgress};
pub(crate) use shutdown_session::RunningShutdownSession;
pub(crate) use shutdown_session::{
    InterruptedExitCandidate, InterruptedExitRecoveryOutcome, RecoveryPreparationFailure,
    ResidentPreparationKey, ResidentRecoveryWindow,
};

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
    ordinary_close_window: Option<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>>,
    ordinary_commands_mounted: bool,
    #[cfg(test)]
    last_ordinary_command_failure: Option<(Option<beryl_model::WindowId>, String)>,
    cancelled_ordinary_close: Option<ordinary_close_session::OrdinaryCloseSession>,
    #[cfg(test)]
    before_ordinary_session_removal: Option<Box<dyn FnOnce(&beryl_home_store::HomeStore) + Send>>,
    #[cfg(test)]
    before_ordinary_command_admission: Option<Box<dyn FnOnce(&beryl_home_store::HomeStore) + Send>>,
    #[cfg(test)]
    before_ordinary_draft_prepare: Option<Box<dyn FnOnce(&beryl_home_store::HomeStore)>>,
    #[cfg(test)]
    before_native_close_restoration: Option<Box<dyn FnOnce(&beryl_home_store::HomeStore) + Send>>,
    exit_availability: Option<gpui::Task<()>>,
    attention_task: Option<gpui::Task<()>>,
    attention_routes: running_threads_attention::RunningThreadsAttentionRoutes,
    observed_home_failure: Option<exit_availability::ObservedHomeFailure>,
    #[cfg(test)]
    exit_wait_stop: Option<futures_channel::oneshot::Sender<futures_channel::oneshot::Sender<()>>>,
    interrupted_exit: Option<shutdown_session::InterruptedExitRecovery>,
    automatic_recovery: Option<shutdown_session::AutomaticInterruptedExitRecovery>,
    final_teardown: Option<final_teardown::FinalTeardown>,
    detached_read_pool: beryl_home_store::TemporaryReadPool,
    #[cfg(test)]
    disable_automatic_recovery: bool,
    #[cfg(test)]
    cancel_recovery_before_publication_validation: bool,
    #[cfg(test)]
    cancel_recovery_after_publication: bool,
    #[cfg(test)]
    cancel_recovery_after_resident_admission: bool,
    #[cfg(test)]
    reject_ordinary_recovery_attachment_after: Option<usize>,
    #[cfg(test)]
    capture_resident_frames: bool,
    #[cfg(test)]
    captured_resident_frame: Option<Box<dyn FnOnce(&mut App)>>,
    #[cfg(test)]
    drop_thread_creation_graph: bool,
    #[cfg(test)]
    returned_claim_graphs: usize,
    #[cfg(test)]
    before_thread_creation_reopen: Option<Box<dyn FnOnce(&ProcessServiceOwner)>>,
    #[cfg(test)]
    reject_first_conversation_widget_release: bool,
    #[cfg(test)]
    exit_waiting_passes: usize,
    #[cfg(test)]
    confirmed_refreshes: usize,
}

pub(crate) struct RunningProcess {
    pub(crate) notification_audio: crate::notification_audio::NotificationAudioLane,
    pub(crate) parent_sound: crate::parent_completion_sound::ParentCompletionSoundOwner,
    configuration: AppServiceConfiguration,
    services: Option<ProcessServiceOwner>,
    pub(crate) windows: PublishedMainWindowRestoreSet,
    appearance: Entity<GpuiAppearanceWindowSet>,
    pub(crate) startup_surface: Option<OwnedStartupSurface>,
    commands: RunningExitCommands,
}

impl RunningProcessOwner {
    pub(crate) fn bind_parent_sound(&self) {
        if let Some(graph) = self
            .process
            .services
            .as_ref()
            .and_then(|services| services.graph())
        {
            graph
                .cas()
                .bind_parent_sound(self.process.parent_sound.sink(
                    self.process.notification_audio.ingress(),
                    graph.state().settings(),
                    graph.cas().service_generation(),
                ));
        }
    }
    pub(crate) fn diagnostic_window_facts(&self, app: &App) -> serde_json::Value {
        let active = app.active_window();
        let mut ids = Vec::new();
        let mut selected = None;
        let mut threadless = !self.process.windows.shells().is_empty();
        for shell in self.process.windows.shells() {
            let Ok(root) = shell.window().read(app) else {
                threadless = false;
                continue;
            };
            let Some(controller) = root.controller() else {
                threadless = false;
                continue;
            };
            ids.push(controller.window_id());
            threadless &= controller.is_threadless();
            if active.is_some_and(|active| active.window_id() == shell.window().window_id()) {
                selected = root.diagnostic_selected_thread(app);
            }
        }
        let home_state = self
            .process
            .services
            .as_ref()
            .and_then(|owner| owner.graph())
            .map(|graph| match graph.home().health().state() {
                beryl_home_store::HomeHealthState::Healthy => "healthy",
                beryl_home_store::HomeHealthState::Failed => "failed",
                beryl_home_store::HomeHealthState::Reopening => "reopening",
                beryl_home_store::HomeHealthState::Opening => "opening",
            })
            .unwrap_or("retained_unavailable");
        serde_json::json!({"mainWindowIds": ids, "selectedThreadId": selected, "threadless": threadless, "homeState": home_state})
    }
    #[cfg(test)]
    pub(crate) fn test_detached_read_usage(&self) -> beryl_home_store::TemporaryReadPoolUsage {
        self.detached_read_pool.usage().unwrap()
    }
    pub(crate) fn start(process: StartedProcess, app: &mut App) -> Rc<RefCell<Self>> {
        Self::start_with_detached_read_limits(process, Default::default(), app)
    }

    pub(crate) fn start_with_detached_read_limits(
        process: StartedProcess,
        limits: beryl_home_store::TemporaryReadPoolLimits,
        app: &mut App,
    ) -> Rc<RefCell<Self>> {
        let owner = Self::construct_running_owner(process, limits, app);
        if let Err(error) = Self::mount_ordinary_commands(&owner, app) {
            owner
                .borrow()
                .process
                .commands
                .set_gate(crate::startup_owner::RunningExitGate::Unavailable, true);
            Self::report_ordinary_command_failure(&owner, None, &error, app);
        }
        owner
    }

    #[cfg(test)]
    pub(crate) fn test_start_unmounted(
        process: StartedProcess,
        app: &mut App,
    ) -> Rc<RefCell<Self>> {
        Self::construct_running_owner(process, Default::default(), app)
    }

    #[cfg(test)]
    pub(crate) fn test_start_unmounted_with_detached_read_limits(
        process: StartedProcess,
        limits: beryl_home_store::TemporaryReadPoolLimits,
        app: &mut App,
    ) -> Rc<RefCell<Self>> {
        Self::construct_running_owner(process, limits, app)
    }

    fn construct_running_owner(
        mut process: StartedProcess,
        limits: beryl_home_store::TemporaryReadPoolLimits,
        app: &mut App,
    ) -> Rc<RefCell<Self>> {
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
                notification_audio: crate::notification_audio::NotificationAudioLane::new(),
                parent_sound: crate::parent_completion_sound::ParentCompletionSoundOwner::new(app),
                configuration: process.configuration,
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
            ordinary_close_window: None,
            ordinary_commands_mounted: false,
            #[cfg(test)]
            last_ordinary_command_failure: None,
            cancelled_ordinary_close: None,
            #[cfg(test)]
            before_ordinary_session_removal: None,
            #[cfg(test)]
            before_ordinary_command_admission: None,
            #[cfg(test)]
            before_ordinary_draft_prepare: None,
            #[cfg(test)]
            before_native_close_restoration: None,
            exit_availability: None,
            attention_task: None,
            attention_routes: running_threads_attention::RunningThreadsAttentionRoutes::default(),
            observed_home_failure: None,
            #[cfg(test)]
            exit_wait_stop: None,
            interrupted_exit: None,
            automatic_recovery: None,
            final_teardown: None,
            detached_read_pool: beryl_home_store::TemporaryReadPool::new(limits),
            #[cfg(test)]
            disable_automatic_recovery: false,
            #[cfg(test)]
            cancel_recovery_before_publication_validation: false,
            #[cfg(test)]
            cancel_recovery_after_publication: false,
            #[cfg(test)]
            cancel_recovery_after_resident_admission: false,
            #[cfg(test)]
            reject_ordinary_recovery_attachment_after: None,
            #[cfg(test)]
            capture_resident_frames: false,
            #[cfg(test)]
            captured_resident_frame: None,
            #[cfg(test)]
            drop_thread_creation_graph: false,
            #[cfg(test)]
            returned_claim_graphs: 0,
            #[cfg(test)]
            before_thread_creation_reopen: None,
            #[cfg(test)]
            reject_first_conversation_widget_release: false,
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
        owner.borrow().bind_parent_sound();
        Self::observe_exit_availability(&owner, app);
        Self::observe_lifecycle_attention(&owner, app);
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
    pub(crate) fn test_process_mut(&mut self) -> &mut RunningProcess {
        &mut self.process
    }

    #[cfg(test)]
    pub(crate) fn test_services(&self) -> &ProcessServiceOwner {
        self.process
            .services
            .as_ref()
            .expect("services retained on GUI")
    }

    #[cfg(test)]
    pub(crate) fn test_take_services(&mut self) -> ProcessServiceOwner {
        self.process
            .services
            .take()
            .expect("test service custody is retained")
    }

    #[cfg(test)]
    pub(crate) fn test_restore_services(&mut self, services: ProcessServiceOwner) {
        assert!(self.process.services.is_none());
        self.process.services = Some(services);
    }

    #[cfg(test)]
    pub(crate) fn test_services_mut(&mut self) -> &mut ProcessServiceOwner {
        self.process
            .services
            .as_mut()
            .expect("services retained on GUI")
    }

    #[cfg(test)]
    pub(crate) fn test_into_process(self) -> StartedProcess {
        assert_ne!(self.startup_cleanup, StartupCleanup::Pending);
        assert!(self.progress.is_none());
        assert!(!self.observing_initial_work);
        assert!(!self.waiting_for_exit);
        StartedProcess {
            configuration: self.process.configuration,
            services: self.process.services.expect("services retained on GUI"),
            windows: self.process.windows,
            appearance: self.process.appearance,
            startup_surface: self.process.startup_surface,
            commands: self.process.commands,
        }
    }
}
