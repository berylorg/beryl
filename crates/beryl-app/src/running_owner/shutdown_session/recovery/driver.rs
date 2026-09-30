use super::*;
use crate::app_services::AppServiceConfiguration;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity};
use std::time::Duration;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
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

    pub(crate) async fn retire_and_settle_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
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
        Self::construct_and_settle_interrupted_exit(owner, request, generation, cancellation, cx)
            .await
    }

    pub(crate) async fn construct_and_settle_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
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
