use super::*;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity};
use std::time::Duration;

impl RunningProcessOwner {
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
