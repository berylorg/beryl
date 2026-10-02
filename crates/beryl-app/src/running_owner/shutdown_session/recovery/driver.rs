use super::*;
use crate::app_services::AppServiceConfiguration;
use crate::main_window::{
    MainWindowComposerRecoveryProgress, MainWindowConversationComposerCloseTicket,
    MainWindowConversationComposerConfigurator, MainWindowShellRoot,
};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity};
use std::time::Duration;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) async fn prepare_and_attach_interrupted_exit_resident_window(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(&mut App) -> Result<resident::ResidentPreparationKey, String>,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::prepare_and_attach_interrupted_exit_resident_pass(
            owner,
            request,
            preparation,
            admit,
            window,
            appearance,
            adapters,
            configurator,
            current,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) async fn prepare_and_attach_interrupted_exit_resident_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(&mut App) -> Result<resident::ResidentPreparationKey, String>,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let mut attached = None;
        Self::attach_and_retain_interrupted_exit_resident_pass(
            owner,
            request,
            preparation,
            admit,
            window,
            appearance,
            &mut attached,
            adapters,
            configurator,
            current,
            cancellation,
            cx,
        )
        .await?;
        Ok(attached.expect("successful resident attachment retained"))
    }

    pub(super) async fn attach_and_retain_interrupted_exit_resident_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(&mut App) -> Result<resident::ResidentPreparationKey, String>,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        attachment: &mut Option<(
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        )>,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if preparation.is_none() {
            cx.update(|app| {
                if !owner
                    .recovery_owner()?
                    .borrow()
                    .process
                    .commands
                    .is_active(request)
                {
                    return Err("Interrupted Exit request changed".to_string());
                }
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                *preparation = Some(admit(app)?);
                #[cfg(test)]
                if std::mem::take(
                    &mut owner
                        .recovery_owner()?
                        .borrow_mut()
                        .cancel_recovery_after_resident_admission,
                ) {
                    cancellation.cancel();
                }
                Ok(())
            })
            .map_err(|error| error.to_string())??;
        } else {
            drop(admit);
        }
        let key = preparation.as_ref().unwrap().clone();
        let mut current = Some(current);
        loop {
            let attached = cx
                .update(|app| {
                    let progress = {
                        let retained_owner = owner.recovery_owner()?;
                        let retained = retained_owner.borrow();
                        if !retained.process.commands.is_active(request) {
                            return Err("Interrupted Exit request changed".into());
                        }
                        retained.interrupted_exit_resident_result(&key)?
                    };
                    if cancellation.is_cancelled() {
                        Self::cancel_interrupted_exit_resident(
                            &owner.recovery_owner()?,
                            &key,
                            app,
                        )?;
                        return Err("Interrupted Exit attachment was cancelled".into());
                    }
                    if progress != MainWindowComposerRecoveryProgress::Ready {
                        return Ok(None);
                    }
                    let attached = window
                        .update(app, |root, window, cx| {
                            let current = current.take().unwrap()(window, cx)?;
                            owner
                                .recovery_owner()?
                                .borrow_mut()
                                .attach_interrupted_exit_resident(
                                    request,
                                    &key,
                                    root,
                                    adapters,
                                    configurator,
                                    current,
                                    window,
                                    cx,
                                )
                        })
                        .map_err(|error| error.to_string())??;
                    preparation.take();
                    *attachment = Some(attached);
                    owner
                        .recovery_owner()?
                        .borrow_mut()
                        .bind_interrupted_exit_appearance(request, window, appearance, app)?;
                    Ok::<_, String>(Some(()))
                })
                .map_err(|error| error.to_string())??;
            if let Some(attached) = attached {
                return Ok(attached);
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
    }

    pub(crate) async fn attach_interrupted_exit_threadless_window(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        retired_home: beryl_model::BerylHomeId,
        retired_generation: HomeGeneration,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::attach_interrupted_exit_threadless_pass(
            owner,
            request,
            retired_home,
            retired_generation,
            window,
            appearance,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) async fn attach_interrupted_exit_threadless_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        retired_home: beryl_model::BerylHomeId,
        retired_generation: HomeGeneration,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit attachment was cancelled".into());
            }
            let window_id = window
                .read_with(app, |root, _| {
                    root.controller()
                        .map(|controller| controller.window_id())
                        .ok_or("Interrupted Exit window controller is unavailable")
                })
                .map_err(|error| error.to_string())??;
            Self::prepare_interrupted_exit_threadless_window(
                &owner.recovery_owner()?,
                request,
                retired_home,
                retired_generation,
                window_id,
                app,
                move |_, result, _| {
                    let _ = sender.send(result);
                },
            )
        })
        .map_err(|error| error.to_string())??;
        let mut source = Some(receiver.await.map_err(|_| {
            "Interrupted Exit window authentication delivery is unavailable".to_string()
        })??);
        cx.update(|app| {
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit attachment was cancelled".into());
            }
            window
                .update(app, |root, window, cx| {
                    owner
                        .recovery_owner()?
                        .borrow_mut()
                        .attach_interrupted_exit_threadless(request, root, &mut source, window, cx)
                })
                .map_err(|error| error.to_string())??;
            owner
                .recovery_owner()?
                .borrow_mut()
                .bind_interrupted_exit_appearance(request, window, appearance, app)
        })
        .map_err(|error| error.to_string())?
    }

    pub(crate) async fn prepare_interrupted_exit_service_graph(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::prepare_interrupted_exit_services(
                &owner.recovery_owner()?,
                request,
                retired,
                configuration,
                at,
                cancellation.clone(),
                app,
                move |_, _| {
                    let _ = sender.send(());
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit service preparation delivery is unavailable")?;
        let result = owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit_services_result(request);
        if let Err(error) = result {
            Self::dispose_returned_interrupted_exit_failure(owner, request, retired, cx).await?;
            let failed_preparation = owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit
                .as_ref()
                .is_some_and(|recovery| {
                    matches!(
                        recovery.settlement.borrow().as_ref(),
                        Some(super::settlement::CandidateSettlement::Services(Err(_)))
                    )
                });
            if failed_preparation {
                owner
                    .recovery_owner()?
                    .borrow_mut()
                    .return_interrupted_exit_preparation_home(request, retired)?;
            }
            return Err(error);
        }
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit service preparation was cancelled".into());
        }
        Ok(())
    }

    pub(super) async fn retire_interrupted_exit_for_preparation(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let receiver = loop {
            let (sender, receiver) = futures_channel::oneshot::channel();
            let admitted = cx
                .update(|app| -> Result<bool, String> {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit retirement was cancelled".into());
                    }
                    {
                        let retained_owner = owner.recovery_owner()?;
                        let mut owner = retained_owner.borrow_mut();
                        owner
                            .process
                            .services
                            .as_ref()
                            .ok_or("The complete service owner is on a worker")?
                            .validate_failed_service_graph_retirement(generation)
                            .map_err(|error| error.to_string())?;
                        if !owner.retire_interrupted_exit_residents(request, app)? {
                            return Ok(false);
                        }
                    }
                    Self::retire_interrupted_exit_graph(
                        &owner.recovery_owner()?,
                        request,
                        generation,
                        app,
                        move |_, _| {
                            let _ = sender.send(());
                        },
                    )?;
                    Ok(true)
                })
                .map_err(|error| error.to_string())??;
            if admitted {
                break receiver;
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        };
        receiver
            .await
            .map_err(|_| "Interrupted Exit retirement delivery is unavailable")?;
        owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit_graph_retirement_result(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit retirement was cancelled".into());
        }
        Ok(())
    }

    pub(crate) async fn retire_and_prepare_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::retire_interrupted_exit_for_preparation(
            owner,
            request,
            generation,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::prepare_interrupted_exit_attempt(
            owner,
            request,
            generation,
            configuration,
            at,
            cancellation,
            cx,
        )
        .await
    }
    pub(super) async fn dispose_returned_interrupted_exit_failure(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let failed_candidate = {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
            owner.interrupted_exit_graph_retirement_result(request)?;
            matches!(
                owner
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .settlement
                    .borrow()
                    .as_ref(),
                Some(super::settlement::CandidateSettlement::Returned { result: Err(_), .. })
            )
        };
        if failed_candidate {
            Self::dispose_interrupted_exit_candidate_failure(owner, request, generation, cx)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn construct_and_settle_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let retry = owner
            .recovery_owner()?
            .borrow()
            .carried_interrupted_exit_resume_retry(request)?;
        loop {
            loop {
                let deadline = owner
                    .recovery_owner()?
                    .borrow()
                    .interrupted_exit_reopen_deadline(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit candidate construction was cancelled".into());
                }
                let Some(delay) = deadline.and_then(|deadline| {
                    deadline.checked_duration_since(std::time::Instant::now())
                }) else {
                    break;
                };
                cx.background_executor()
                    .timer(delay.min(Duration::from_millis(50)))
                    .await;
            }
            let (sender, receiver) = futures_channel::oneshot::channel();
            cx.update(|app| {
                Self::construct_interrupted_exit_candidate(
                    &owner.recovery_owner()?,
                    request,
                    retired,
                    cancellation.clone(),
                    app,
                    move |_, _| {
                        let _ = sender.send(());
                    },
                )
            })
            .map_err(|error| error.to_string())??;
            receiver
                .await
                .map_err(|_| "Interrupted Exit construction delivery is unavailable")?;
            if owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit_reopen_deadline(request)?
                .is_some()
            {
                continue;
            }
            owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit_construction_result(request)?;
            break;
        }

        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit_construction_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit candidate settlement was cancelled".into());
            }
            Self::settle_constructed_exit_candidate(
                &owner.recovery_owner()?,
                request,
                app,
                move |_, _| {
                    let _ = sender.send(());
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit candidate settlement delivery is unavailable")?;

        if let Some(retry) = retry {
            let receiver = cx
                .update(|app| {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit candidate settlement was cancelled".into());
                    }
                    if owner
                        .recovery_owner()?
                        .borrow()
                        .interrupted_exit_candidate_result(request)
                        .is_ok()
                    {
                        return Ok(None);
                    }
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    Self::retry_interrupted_exit_resume_pass(
                        &owner.recovery_owner()?,
                        request,
                        retry,
                        app,
                        move |_, _| {
                            let _ = sender.send(());
                        },
                    )?;
                    Ok::<_, String>(Some(receiver))
                })
                .map_err(|error| error.to_string())??;
            if let Some(receiver) = receiver {
                receiver
                    .await
                    .map_err(|_| "Interrupted Exit resume retry delivery is unavailable")?;
            }
        }

        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit_candidate_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit process-work settlement was cancelled".into());
            }
            Self::settle_interrupted_exit_process_work(
                &owner.recovery_owner()?,
                request,
                cancellation.clone(),
                app,
                move |_, _| {
                    let _ = sender.send(());
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit process-work settlement delivery is unavailable")?;
        owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit_candidate_result(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit process-work settlement was cancelled".into());
        }
        Ok(())
    }
}
