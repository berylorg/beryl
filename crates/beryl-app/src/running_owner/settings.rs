use super::*;
use crate::{
    settings_service::AppliedSettingsReader,
    settings_window::SettingsWindowController,
    startup_owner::RunningExitGate,
    theme_runtime::{RepositoryAppearanceResult, SettingsThemeOutcome, SettingsThemeResult},
};
use std::sync::Arc;

impl RunningProcessOwner {
    pub(super) fn project_settings(owner: &Rc<RefCell<Self>>, app: &mut App) {
        if owner.borrow().settings_appearance_busy {
            return;
        }
        let bindings = {
            let retained = owner.borrow();
            retained
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .and_then(|graph| {
                    Some((
                        Arc::clone(graph.settings()?),
                        graph.settings_theme_reader()?,
                        graph.current_appearance()?,
                        retained.process.appearance.clone(),
                        graph.home().health().state() == beryl_home_store::HomeHealthState::Healthy,
                        graph.settings_theme_failure().is_some(),
                    ))
                })
        };
        let Some((service, themes, appearance, window_set, home_available, appearance_failed)) =
            bindings
        else {
            return;
        };
        let reader = service.applied_reader();
        let changed = owner
            .borrow()
            .settings_source
            .as_ref()
            .is_none_or(|old| !old.same_source(&reader));
        if changed {
            let existing = owner.borrow().settings_window.clone();
            let result = if let Some(controller) = existing {
                {
                    let retained = owner.borrow();
                    if let Some(retirement) = retained
                        .process
                        .services
                        .as_ref()
                        .and_then(|services| services.settings_theme_retirement())
                    {
                        controller.update(app, |controller, cx| {
                            controller.observe_theme_retirement(retirement, cx)
                        });
                    }
                }
                controller
                    .update(app, |controller, cx| {
                        controller.replace_service(service, themes, cx)
                    })
                    .map(|_| controller)
            } else {
                SettingsWindowController::preheat(service, themes, window_set, appearance, app)
            };
            match result {
                Ok(controller) => {
                    controller.update(app, |controller, cx| {
                        controller.set_home_available(home_available, cx);
                        if appearance_failed {
                            controller.report_appearance_failure(cx);
                        } else {
                            controller.clear_appearance_failure(cx);
                        }
                    });
                    if owner.borrow().settings_window.is_none() {
                        let window = controller.read(app).native_window().cloned();
                        let receipt = window
                            .ok_or_else(|| "Settings native window is unavailable".to_owned())
                            .and_then(|window| {
                                window
                                    .window_handle()
                                    .update(app, |_, window, _| {
                                        window.observe_windows_native_destruction()
                                    })
                                    .map_err(|error| error.to_string())
                                    .and_then(|result| result.map_err(|error| error.to_string()))
                            });
                        match receipt {
                            Ok(receipt) => owner.borrow_mut().settings_destruction = Some(receipt),
                            Err(error) => {
                                let _ = controller
                                    .update(app, |controller, cx| controller.shutdown(cx));
                                owner.borrow_mut().settings_source = Some(reader);
                                tracing::error!(%error, "Settings native lifecycle registration failed");
                                return;
                            }
                        }
                    }
                    let commands = owner.borrow().process.commands.clone();
                    let observer = app.observe(&controller, move |controller, app| {
                        commands.set_gate(
                            RunningExitGate::SettingsReconciliation,
                            controller.read(app).exit_is_gated(),
                        );
                    });
                    crate::settings_command::SettingsCommandRoute::install(
                        SettingsWindowController::command(&controller),
                        app,
                    );
                    let mut retained = owner.borrow_mut();
                    retained.settings_window = Some(controller);
                    retained.settings_source = Some(reader.clone());
                    retained.settings_observer = Some(observer);
                }
                Err(error) => {
                    if matches!(
                        error,
                        crate::settings_window::SettingsWindowMountError::Busy
                    ) {
                        return;
                    }
                    owner.borrow_mut().settings_source = Some(reader.clone());
                    tracing::warn!(%error, "Settings service could not mount or rebind");
                    return;
                }
            }
        }
        Self::publish_settings_autosave(&reader, app);
        let controller = owner.borrow().settings_window.clone();
        let Some(controller) = controller else {
            return;
        };
        controller.update(app, |controller, cx| {
            controller.set_home_available(home_available, cx)
        });
        {
            let retained = owner.borrow();
            if !retained.ordinary_commands_mounted
                || retained.interrupted_exit.is_some()
                || retained.progress.is_some()
                || retained.shutdown.is_some()
                || retained.exit_requested()
                || retained.final_teardown.is_some()
            {
                return;
            }
        }
        let (confirmed, retry) = controller.update(app, |controller, _| {
            (
                controller.take_confirmed_theme(),
                controller.take_appearance_retry(),
            )
        });
        let completion = owner
            .borrow()
            .process
            .services
            .as_ref()
            .and_then(|services| services.graph())
            .and_then(|graph| graph.take_settings_theme_completion());
        let mut watch = Ok(Vec::new());
        if confirmed.is_none() && completion.is_none() && !retry {
            if home_available && !controller.read(app).exit_is_gated() {
                if let Some(graph) = owner
                    .borrow_mut()
                    .process
                    .services
                    .as_mut()
                    .and_then(|services| services.graph_mut())
                {
                    if graph.sync_settings_theme_identity().is_err() {
                        controller.update(app, |controller, cx| {
                            controller.report_appearance_failure(cx)
                        });
                    }
                    watch = graph.take_settings_theme_hints();
                }
            }
            if watch.as_ref().is_ok_and(Vec::is_empty) {
                owner.borrow().process.commands.set_gate(
                    RunningExitGate::SettingsReconciliation,
                    controller.read(app).exit_is_gated(),
                );
                return;
            }
        }
        controller.update(app, |controller, cx| {
            controller.set_appearance_busy(true, cx)
        });
        let (mut services, appearance_home) = {
            let mut retained = owner.borrow_mut();
            retained.settings_appearance_busy = true;
            retained
                .process
                .commands
                .set_gate(RunningExitGate::SettingsReconciliation, true);
            retained.settings_appearance_readers = retained
                .process
                .services
                .as_ref()
                .map(super::SettingsAppearanceReaders::capture);
            let services = retained
                .process
                .services
                .take()
                .expect("observed Settings service graph");
            let home = services.graph().and_then(|graph| {
                graph
                    .home()
                    .health()
                    .generation()
                    .map(|generation| (graph.home().home_id(), generation))
            });
            (services, home)
        };
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let graph = services
                    .graph_mut()
                    .ok_or("Settings appearance graph is unavailable")?;
                let home = graph.home().service_reference();
                let runtime = graph
                    .theme()
                    .ok_or("Settings appearance runtime is unavailable")?;
                if let Err(failure) = &watch {
                    runtime.retire();
                    return Err(format!("Theme change subscription failed: {failure:?}"));
                }
                let mut resolved = false;
                if let Some(confirmed) = confirmed {
                    let result = runtime
                        .consume_settings_outcome(&home, SettingsThemeOutcome::Committed(confirmed))
                        .map_err(|error| format!("{error:?}"))?;
                    if matches!(result, SettingsThemeResult::Retained) {
                        return Err("Settings appearance retained its prior publication".into());
                    }
                    resolved = true;
                }
                if let Some(completion) = completion {
                    let result = runtime
                        .consume_repository_completion(&home, completion)
                        .map_err(|error| format!("{error:?}"))?;
                    if matches!(result, RepositoryAppearanceResult::Retained(_)) {
                        return Err("Repository appearance retained its prior publication".into());
                    }
                    resolved |= matches!(
                        result,
                        RepositoryAppearanceResult::Published
                            | RepositoryAppearanceResult::HiddenBaseReplaced
                    );
                }
                if retry
                    && matches!(
                        runtime
                            .retry_current_durable(&home)
                            .map_err(|error| format!("{error:?}"))?,
                        RepositoryAppearanceResult::Retained(_)
                    )
                {
                    return Err("Appearance retry retained its prior publication".into());
                }
                resolved |= retry;
                graph
                    .sync_settings_theme_identity()
                    .map_err(|error| format!("{error:?}"))?;
                if let Ok(hints) = watch {
                    if !hints.is_empty()
                        && matches!(
                            graph
                                .consume_settings_theme_hints(&hints)
                                .map_err(|error| format!("{error:?}"))?
                                .appearance(),
                            RepositoryAppearanceResult::Retained(_)
                        )
                    {
                        return Err("External theme edit retained its prior appearance".into());
                    }
                    if !hints.is_empty() {
                        let diagnostics = graph
                            .theme()
                            .ok_or("Settings appearance runtime is unavailable")?
                            .diagnostics();
                        resolved |= diagnostics.last_failure().is_none()
                            && !diagnostics.refresh_reread_needed()
                            && diagnostics.appearance().is_some_and(|appearance| {
                                !appearance.pending_durable_application()
                            });
                    }
                }
                Ok::<_, String>(resolved)
            }))
            .unwrap_or_else(|_| Err("Settings appearance worker unwound".into()));
            (services, result)
        });
        let retained = Rc::clone(owner);
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            {
                let mut owner = retained.borrow_mut();
                owner.process.services = Some(services);
                owner.settings_appearance_readers = None;
                owner.settings_appearance_busy = false;
            }
            let _ = cx.update(|app| {
                controller.update(app, |controller, cx| {
                    if result.is_err() {
                        controller.report_appearance_failure(cx);
                    } else if result == Ok(true) {
                        controller.clear_appearance_failure(cx);
                    }
                    controller.set_appearance_busy(false, cx);
                });
                Self::project_settings(&retained, app);
                if let Some((home, generation)) = appearance_home {
                    crate::main_window::MainWindowCreationOwner::resume_after_settings_appearance(
                        &retained, home, generation, app,
                    );
                }
            });
        })
        .detach();
    }

    fn publish_settings_autosave(reader: &AppliedSettingsReader, app: &mut App) {
        if let Err(error) = crate::settings_autosave::publish_applied(reader, app) {
            tracing::warn!(%error, "Applied Settings autosave projection is unavailable");
        }
    }
}
