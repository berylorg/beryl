use super::*;
use crate::main_window::MainWindowShellRoot;
use beryl_home_store::CommandCancellation;
use gpui::{App, AsyncApp, WindowHandle};
use resident_windows_driver::ResidentRecoveryConfigurator;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    async fn recover_resident_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let appearance =
            Self::retained_interrupted_exit_appearance(owner, request, &cancellation, cx)?;
        let (threadless, home, retired, generation) = cx
            .update(|app| -> Result<_, String> {
                use crate::theme_runtime::AppearancePublicationTarget;
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                owner.interrupted_exit_graph_retirement_result(request)?;
                let snapshot = appearance.read(app).target().snapshot();
                let home = snapshot.current.prepared().home();
                let retired = owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("The complete service owner is on a worker")?
                    .retired_service_generation_for_home_return(home.home_id())
                    .map_err(|error| error.to_string())?;
                Ok((
                    owner.interrupted_exit_threadless_window(app)?,
                    home.home_id(),
                    retired,
                    home.home_generation(),
                ))
            })
            .map_err(|error| error.to_string())??;
        if let Some(window) = threadless {
            Self::continue_interrupted_exit_threadless(
                owner,
                request,
                home,
                retired,
                generation,
                window,
                cancellation,
                cx,
            )
            .await
        } else {
            Self::continue_interrupted_exit_selected_windows(
                owner,
                request,
                retired,
                generation,
                cancellation,
                cx,
            )
            .await
        }
    }

    pub(crate) async fn recover_prepared_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let publication_returned = {
            let retained_owner = owner.recovery_owner()?;
            let mut owner = retained_owner.borrow_mut();
            let _driver = owner.reserve_interrupted_exit_driver(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit preparation was cancelled".into());
            }
            owner
                .interrupted_exit
                .as_ref()
                .unwrap()
                .publication
                .borrow()
                .is_some()
        };
        if publication_returned {
            return Self::recover_published_interrupted_exit(owner, request, cancellation, cx)
                .await;
        }
        let first = owner
            .recovery_owner()?
            .borrow()
            .first_conversation_window()?;
        if let Some(window) = first {
            let retired = {
                let retained = owner.recovery_owner()?;
                let mut retained = retained.borrow_mut();
                let prepared = retained.interrupted_exit_appearance(request)?;
                retained
                    .process
                    .services
                    .as_ref()
                    .ok_or("first conversation service owner is unavailable")?
                    .retired_service_generation_for_home_return(
                        prepared.prepared().home().home_id(),
                    )
                    .map_err(|e| e.to_string())?
            };
            return Self::recover_first_conversation(
                owner,
                request,
                retired,
                window,
                SyndicTimestamp::from_unix_millis(0),
                cancellation,
                |_| {},
                cx,
            )
            .await;
        }
        let (threadless, retired) = cx
            .update(|app| -> Result<_, String> {
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                let threadless = owner.interrupted_exit_threadless_window(app)?;
                let recovery = owner.interrupted_exit.as_ref().unwrap();
                let retained = if threadless.is_some() {
                    recovery.threadless_appearance.is_some()
                } else {
                    recovery
                        .selected_windows
                        .as_ref()
                        .ok_or("Interrupted Exit selected recovery is not retained")?
                        .try_borrow()
                        .map_err(|_| "Interrupted Exit selected recovery is busy")?
                        .has_appearance()
                };
                if retained {
                    return Ok((threadless, None));
                }
                let prepared = owner.interrupted_exit_appearance(request)?;
                let retired = owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("The complete service owner is on a worker")?
                    .retired_service_generation_for_home_return(
                        prepared.prepared().home().home_id(),
                    )
                    .map_err(|error| error.to_string())?;
                Ok((threadless, Some(retired)))
            })
            .map_err(|error| error.to_string())??;
        let Some(retired) = retired else {
            return Self::recover_resident_interrupted_exit(owner, request, cancellation, cx).await;
        };
        if let Some(window) = threadless {
            Self::complete_prepared_interrupted_exit_threadless(
                owner,
                request,
                retired,
                window,
                cancellation,
                cx,
            )
            .await
        } else {
            Self::complete_prepared_interrupted_exit_selected_windows(
                owner,
                request,
                retired,
                cancellation,
                cx,
            )
            .await
        }
    }

    async fn recover_published_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let appearance =
            Self::retained_interrupted_exit_appearance(owner, request, &cancellation, cx)?;
        let activated = {
            let retained_owner = owner.recovery_owner()?;
            let mut owner = retained_owner.borrow_mut();
            let _driver = owner.reserve_interrupted_exit_driver(request)?;
            owner.interrupted_exit_publication_result(request)?;
            if owner.process.services.is_none() {
                return Err("The complete service owner is on a worker".into());
            }
            let activation = owner
                .interrupted_exit
                .as_ref()
                .unwrap()
                .theme_activation
                .borrow();
            match activation.as_ref() {
                Some(result) => {
                    result.clone()?;
                    true
                }
                None => false,
            }
        };
        if activated {
            Self::bind_and_complete_interrupted_exit(owner, request, &appearance, cancellation, cx)
                .await
        } else {
            Self::activate_and_complete_interrupted_exit(
                owner,
                request,
                &appearance,
                cancellation,
                cx,
            )
            .await
        }
    }

    fn retained_interrupted_exit_appearance(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        cancellation: &CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>, String> {
        let threadless = cx
            .update(|app| -> Result<_, String> {
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                Ok(owner.first_conversation_window()?.is_some()
                    || owner.interrupted_exit_threadless_window(app)?.is_some())
            })
            .map_err(|error| error.to_string())??;
        if threadless {
            owner
                .recovery_owner()?
                .borrow_mut()
                .interrupted_exit_threadless_appearance(request)
        } else {
            owner
                .recovery_owner()?
                .borrow_mut()
                .interrupted_exit_selected_appearance(request)
        }
    }

    pub(crate) async fn recover_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        configure: impl FnMut(
            WindowHandle<MainWindowShellRoot>,
        ) -> Result<ResidentRecoveryConfigurator, String>,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let first = owner
            .recovery_owner()?
            .borrow()
            .first_conversation_window()?;
        if let Some(window) = first {
            let retired = owner
                .recovery_owner()?
                .borrow()
                .recovery_supervisor_generation(request)?;
            return Self::recover_first_conversation(
                owner,
                request,
                retired,
                window,
                at,
                cancellation,
                failed,
                cx,
            )
            .await;
        }
        let (threadless, retired) = cx
            .update(|app| -> Result<_, String> {
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                let threadless = owner.interrupted_exit_threadless_window(app)?;
                let retired = owner
                    .process
                    .services
                    .as_ref()
                    .and_then(|services| services.graph())
                    .ok_or("Interrupted Exit original service graph is unavailable")?
                    .home()
                    .health()
                    .generation()
                    .ok_or("Interrupted Exit original service generation is unavailable")?;
                owner
                    .process
                    .services
                    .as_ref()
                    .unwrap()
                    .validate_failed_service_graph_retirement(retired)
                    .map_err(|error| format!("{error:?}"))?;
                drop(_driver);
                if threadless.is_none()
                    && owner
                        .interrupted_exit
                        .as_ref()
                        .unwrap()
                        .selected_windows
                        .is_none()
                {
                    owner.retain_interrupted_exit_selected_windows(request, app, configure)?;
                }
                Ok((threadless, retired))
            })
            .map_err(|error| error.to_string())??;
        if let Some(window) = threadless {
            Self::recover_interrupted_exit_threadless(
                owner,
                request,
                retired,
                window,
                at,
                cancellation,
                failed,
                cx,
            )
            .await
        } else {
            Self::recover_interrupted_exit_selected_windows(
                owner,
                request,
                retired,
                at,
                cancellation,
                failed,
                cx,
            )
            .await
        }
    }

    pub(crate) async fn recover_retired_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let first = owner
            .recovery_owner()?
            .borrow()
            .first_conversation_window()?;
        if let Some(window) = first {
            let retired = owner
                .recovery_owner()?
                .borrow()
                .process
                .services
                .as_ref()
                .ok_or("first conversation service owner is unavailable")?
                .retired_service_generation()
                .map_err(|e| e.to_string())?;
            return Self::recover_first_conversation(
                owner,
                request,
                retired,
                window,
                at,
                cancellation,
                failed,
                cx,
            )
            .await;
        }
        let (threadless, retired) = cx
            .update(|app| -> Result<_, String> {
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                owner.interrupted_exit_graph_retirement_result(request)?;
                let retired = owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("Interrupted Exit service owner is unavailable")?
                    .retired_service_generation()
                    .map_err(|error| format!("{error:?}"))?;
                owner.validate_interrupted_exit_preparation(request, retired)?;
                Ok((owner.interrupted_exit_threadless_window(app)?, retired))
            })
            .map_err(|error| error.to_string())??;
        if let Some(window) = threadless {
            Self::prepare_retired_interrupted_exit_threadless(
                owner,
                request,
                retired,
                window,
                at,
                cancellation,
                failed,
                cx,
            )
            .await
        } else {
            Self::prepare_retired_interrupted_exit_selected_windows(
                owner,
                request,
                retired,
                at,
                cancellation,
                failed,
                cx,
            )
            .await
        }
    }

    fn interrupted_exit_threadless_window(
        &self,
        app: &App,
    ) -> Result<Option<WindowHandle<MainWindowShellRoot>>, String> {
        let shells = self.process.windows.shells();
        if shells.is_empty() {
            return Err("Interrupted Exit requires retained windows".into());
        }
        let mut threadless = None;
        for shell in shells {
            let window = shell.window();
            let root = window.read(app).map_err(|error| error.to_string())?;
            let controller = root
                .controller()
                .ok_or("Interrupted Exit window controller is unavailable")?;
            if controller.is_threadless() {
                if shells.len() != 1 {
                    return Err(
                        "Interrupted Exit requires the sole retained threadless window".into(),
                    );
                }
                threadless = Some(window);
            } else if controller.composer_mount().is_none() {
                return Err("Interrupted Exit requires selected windows".into());
            }
        }
        Ok(threadless)
    }
}
