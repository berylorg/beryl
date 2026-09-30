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
        owner: &Rc<RefCell<Self>>,
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
        if preparation.is_none() {
            cx.update(|app| {
                if !owner.borrow().process.commands.is_active(request) {
                    return Err("Interrupted Exit request changed".to_string());
                }
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                *preparation = Some(admit(app)?);
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
                        let retained = owner.borrow();
                        if !retained.process.commands.is_active(request) {
                            return Err("Interrupted Exit request changed".into());
                        }
                        retained.interrupted_exit_resident_result(&key)?
                    };
                    if cancellation.is_cancelled() {
                        Self::cancel_interrupted_exit_resident(owner, &key, app)?;
                        return Err("Interrupted Exit attachment was cancelled".into());
                    }
                    if progress != MainWindowComposerRecoveryProgress::Ready {
                        return Ok(None);
                    }
                    let attached = window
                        .update(app, |root, window, cx| {
                            let current = current.take().unwrap()(window, cx)?;
                            owner.borrow_mut().attach_interrupted_exit_resident(
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
                    owner
                        .borrow_mut()
                        .bind_interrupted_exit_appearance(request, window, appearance, app)?;
                    Ok::<_, String>(Some(attached))
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
        owner: &Rc<RefCell<Self>>,
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
                owner,
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
                    owner.borrow_mut().attach_interrupted_exit_threadless(
                        request,
                        root,
                        &mut source,
                        window,
                        cx,
                    )
                })
                .map_err(|error| error.to_string())??;
            owner
                .borrow_mut()
                .bind_interrupted_exit_appearance(request, window, appearance, app)
        })
        .map_err(|error| error.to_string())?
    }

    pub(crate) async fn prepare_interrupted_exit_service_graph(
        owner: &Rc<RefCell<Self>>,
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
                owner,
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
        owner.borrow().interrupted_exit_services_result(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit service preparation was cancelled".into());
        }
        Ok(())
    }

    pub(crate) async fn retire_and_prepare_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = {
            let mut owner = owner.borrow_mut();
            if !owner.process.commands.is_active(request) {
                return Err("Interrupted Exit request changed".into());
            }
            let recovery = owner
                .interrupted_exit
                .as_mut()
                .ok_or("No reported failed Exit")?;
            if !Rc::ptr_eq(&recovery.request, &request.identity()) {
                return Err("Interrupted Exit request changed".into());
            }
            if recovery.preparation_driver.upgrade().is_some() {
                return Err("Interrupted Exit preparation is already being driven".into());
            }
            let driver = Rc::new(());
            recovery.preparation_driver = Rc::downgrade(&driver);
            driver
        };
        let receiver = loop {
            let (sender, receiver) = futures_channel::oneshot::channel();
            let admitted = cx
                .update(|app| -> Result<bool, String> {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit retirement was cancelled".into());
                    }
                    {
                        let mut owner = owner.borrow_mut();
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
                        owner,
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
            .borrow()
            .interrupted_exit_graph_retirement_result(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit retirement was cancelled".into());
        }
        Self::construct_and_settle_interrupted_exit(
            owner,
            request,
            generation,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::prepare_interrupted_exit_service_graph(
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

    pub(crate) async fn construct_and_settle_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        loop {
            loop {
                let deadline = owner.borrow().interrupted_exit_reopen_deadline(request)?;
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
                    owner,
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
                .borrow()
                .interrupted_exit_reopen_deadline(request)?
                .is_some()
            {
                continue;
            }
            owner
                .borrow()
                .interrupted_exit_construction_result(request)?;
            break;
        }

        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            owner
                .borrow()
                .interrupted_exit_construction_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit candidate settlement was cancelled".into());
            }
            Self::settle_constructed_exit_candidate(owner, request, app, move |_, _| {
                let _ = sender.send(());
            })
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit candidate settlement delivery is unavailable")?;

        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            owner.borrow().interrupted_exit_candidate_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit process-work settlement was cancelled".into());
            }
            Self::settle_interrupted_exit_process_work(
                owner,
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
        owner.borrow().interrupted_exit_candidate_result(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit process-work settlement was cancelled".into());
        }
        Ok(())
    }

    pub(crate) async fn attach_and_complete_interrupted_exit_threadless(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired_home: beryl_model::BerylHomeId,
        retired: HomeGeneration,
        generation: HomeGeneration,
        window: gpui::WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        Self::attach_interrupted_exit_threadless_window(
            owner,
            request,
            retired_home,
            retired,
            window,
            appearance,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::publish_and_complete_interrupted_exit(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await
    }

    pub(crate) async fn publish_and_complete_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        appearance: &Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::publish_interrupted_exit_services(
                owner,
                request,
                retired,
                generation,
                appearance,
                cancellation.clone(),
                app,
                move |_, result, _| {
                    let _ = sender.send(result);
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit publication delivery is unavailable")??;

        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::activate_interrupted_exit_theme(
                owner,
                request,
                appearance,
                cancellation.clone(),
                app,
                move |_, result, _| {
                    let _ = sender.send(result);
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit theme activation delivery is unavailable")??;

        cx.update(|app| {
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit completion was cancelled".into());
            }
            owner
                .borrow_mut()
                .bind_interrupted_exit_process(request, appearance, app)
        })
        .map_err(|error| error.to_string())??;
        Self::await_interrupted_exit_completion(owner, request, cancellation, cx).await
    }

    pub(crate) async fn await_interrupted_exit_completion(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        loop {
            let complete = cx
                .update(|app| {
                    if cancellation.is_cancelled() {
                        return Err("Interrupted Exit completion was cancelled".into());
                    }
                    Self::complete_interrupted_exit(owner, request, app)
                })
                .map_err(|error| error.to_string())??;
            if complete {
                return Ok(());
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
    }
}
