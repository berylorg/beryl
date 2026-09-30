use super::*;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity};
use std::time::Duration;

impl RunningProcessOwner {
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
