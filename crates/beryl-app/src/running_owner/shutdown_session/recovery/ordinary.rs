use super::identity::OrdinaryHomeRecoveryKey;
use super::*;
use crate::running_owner::shutdown_drafts::RunningShutdownDrafts;
use crate::running_owner::unchanged_running::{RunningWindowFacts, UnchangedRunning};
use crate::startup_owner::RunningExitGate;
use beryl_home_store::{HomeGeneration, HomeHealthState};

impl RunningProcessOwner {
    pub(in crate::running_owner) fn observe_running_home_failure(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) {
        let (home, generation) = {
            let retained = owner.borrow();
            #[cfg(test)]
            if retained.disable_automatic_recovery {
                return;
            }
            if retained.interrupted_exit.is_some()
                || retained.shutdown.is_some()
                || retained.confirmation.is_some()
                || retained.progress.is_some()
                || retained.cancelled_ordinary_close.is_some()
                || retained.final_teardown.is_some()
                || retained.observing_initial_work
                || retained.process.commands.lifecycle_admitted()
                || !matches!(retained.startup_cleanup, StartupCleanup::Settled)
            {
                return;
            }
            let Some(graph) = retained.process.services.as_ref().and_then(|s| s.graph()) else {
                return;
            };
            let health = graph.home().health();
            if health.state() != HomeHealthState::Failed {
                return;
            }
            let Some(generation) = health.generation() else {
                return;
            };
            (graph.home().home_id(), generation)
        };
        if !crate::main_window::MainWindowCreationOwner::fence_running_home_recovery(
            home, generation, app,
        )
        .unwrap_or(false)
        {
            return;
        }
        let admission = Self::capture_running_home_failure(owner, generation, app);
        match admission {
            Ok(Some(key)) => Self::start_recovery_task(owner, key, app),
            Err(_) => {
                let identity = owner
                    .borrow()
                    .interrupted_exit
                    .as_ref()
                    .filter(|recovery| recovery.ordinary)
                    .map(|recovery| recovery.request.clone());
                if let Some(identity) = identity {
                    Self::start_recovery_task(owner, OrdinaryHomeRecoveryKey(identity), app);
                }
            }
            Ok(None) => {}
        }
    }

    fn capture_running_home_failure(
        owner: &Rc<RefCell<Self>>,
        generation: HomeGeneration,
        app: &mut App,
    ) -> Result<Option<OrdinaryHomeRecoveryKey>, String> {
        let (home, path, windows, members) = {
            let retained = owner.borrow();
            if retained.interrupted_exit.is_some() {
                return Ok(None);
            }
            if retained.process.commands.lifecycle_admitted() || retained.shutdown.is_some() {
                return Err("a lifecycle request already owns failed-home recovery".into());
            }
            let services = retained
                .process
                .services
                .as_ref()
                .ok_or("failed service custody is on a worker")?;
            services
                .validate_failed_service_graph_retirement(generation)
                .map_err(|e| e.to_string())?;
            let graph = services
                .graph()
                .ok_or("failed service graph is unavailable")?;
            let windows = retained
                .process
                .windows
                .shells()
                .iter()
                .map(|s| s.window())
                .collect::<Vec<_>>();
            if windows.is_empty() || windows.len() > beryl_state::MAX_RESTORABLE_WINDOWS {
                return Err(
                    "ordinary home recovery requires the complete bounded window set".into(),
                );
            }
            let members = windows
                .iter()
                .map(|window| {
                    let root = window.read(app).map_err(|e| e.to_string())?;
                    let controller = root
                        .controller()
                        .ok_or("ordinary home recovery controller is unavailable")?;
                    let selection = controller
                        .composer_mount()
                        .map(|mount| {
                            mount
                                .read(app)
                                .contribution()
                                .ok_or("ordinary home recovery resident is unavailable")
                                .map(|resident| resident.read(app).selection_identity().claim())
                        })
                        .transpose()?;
                    if controller.is_threadless() != selection.is_none() {
                        return Err(
                            "ordinary home recovery selection and shell disagree".to_string()
                        );
                    }
                    Ok(RunningWindowFacts {
                        window: controller.window_id(),
                        revision: controller.recovery_window_revision()?,
                        placement: controller.placement().clone(),
                        selection,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            (
                graph.home().home_id(),
                graph.home().canonical_path().to_owned(),
                windows,
                members,
            )
        };
        let identity = Rc::new(());
        let observed_focus = owner
            .borrow()
            .observed_home_failure
            .as_ref()
            .filter(|failure| failure.generation == generation)
            .map(|failure| failure.focus.clone());
        let focus = if let Some(observed) = observed_focus {
            observed
                .into_iter()
                .filter(|(handle, _)| windows.contains(handle))
                .filter_map(|(handle, focus)| focus.map(|focus| (handle, focus)))
                .collect()
        } else {
            windows
                .iter()
                .map(|handle| {
                    handle
                        .update(app, |root, window, cx| {
                            root.capture_running_recovery_focus(window, cx)
                                .map(|focus| (*handle, focus))
                        })
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?
                .into_iter()
                .flatten()
                .collect()
        };
        let drafts = Rc::new(RefCell::new(RunningShutdownDrafts::empty_recovery()));
        {
            let mut retained = owner.borrow_mut();
            retained
                .process
                .commands
                .set_gate(RunningExitGate::HomeUnavailable, true);
            retained.interrupted_exit = Some(InterruptedExitRecovery {
                request: identity.clone(),
                ordinary: true,
                drafts: Some(drafts.clone()),
                focus,
                session: Rc::new(RefCell::new(Some(
                    RunningShutdownSession::UnchangedRunning(UnchangedRunning::new(
                        home, path, members,
                    )),
                ))),
                previous_resume: Rc::new(RefCell::new(None)),
                settlement: Rc::new(RefCell::new(None)),
                service_validation: Rc::new(RefCell::new(None)),
                theme_activation: Rc::new(RefCell::new(None)),
                publication: Rc::new(RefCell::new(None)),
                retirement: Rc::new(RefCell::new(None)),
                resident: None,
                pending_resident_frame: None,
                driver: std::rc::Weak::new(),
                selected_windows: None,
                threadless_appearance: None,
                reopen_schedule: Default::default(),
                reopen_deadline: None,
                residents: Vec::new(),
            });
        }
        for window in windows {
            window
                .update(app, |root, window, cx| {
                    root.set_notices_inert(true, window, cx);
                    root.set_shutdown_interaction_gated(true, cx)
                })
                .map_err(|e| e.to_string())??;
            drafts.borrow_mut().add_recovery_window(window, app);
        }
        let residents = drafts.borrow().recovery_residents();
        owner
            .borrow_mut()
            .interrupted_exit
            .as_mut()
            .unwrap()
            .residents = residents;
        drafts.borrow().require_complete_capture()?;
        Ok(Some(OrdinaryHomeRecoveryKey(identity)))
    }

    #[cfg(test)]
    pub(crate) fn test_observe_running_home_failure(owner: &Rc<RefCell<Self>>, app: &mut App) {
        Self::observe_running_home_failure(owner, app);
    }

    #[cfg(test)]
    pub(crate) fn test_running_home_recovery_identity(&self) -> Option<Rc<()>> {
        self.interrupted_exit
            .as_ref()
            .filter(|r| r.ordinary)
            .map(|r| r.request.clone())
    }

    #[cfg(test)]
    pub(crate) fn test_reject_ordinary_recovery_attachment_after(&mut self, attached: usize) {
        self.reject_ordinary_recovery_attachment_after = Some(attached);
    }

    #[cfg(test)]
    pub(crate) async fn test_settle_running_home_recovery_cancellation(
        owner: &Rc<RefCell<Self>>,
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        use crate::theme_runtime::AppearancePublicationTarget;
        let (key, retired) = cx
            .update(|app| {
                let retained = owner.borrow();
                let identity = retained
                    .test_running_home_recovery_identity()
                    .ok_or("ordinary home recovery custody is unavailable")?;
                let appearance = retained.process.appearance.read(app).target().snapshot();
                let home = appearance.current.prepared().home().home_id();
                let retired = retained
                    .process
                    .services
                    .as_ref()
                    .ok_or("ordinary service owner is on a worker")?
                    .retired_service_generation_for_home_return(home)
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((OrdinaryHomeRecoveryKey(identity), retired))
            })
            .map_err(|e| e.to_string())??;
        Self::settle_automatic_interrupted_exit_cancellation(owner, &key, retired, cx).await
    }

    #[cfg(test)]
    pub(crate) fn test_observe_running_home_failure_generation(
        owner: &Rc<RefCell<Self>>,
        generation: HomeGeneration,
        app: &mut App,
    ) {
        let current = owner
            .borrow()
            .process
            .services
            .as_ref()
            .and_then(|s| s.graph())
            .is_some_and(|graph| graph.home().health().generation() == Some(generation));
        if current {
            Self::observe_running_home_failure(owner, app);
        }
    }
}
