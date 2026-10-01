use super::*;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::AsyncApp;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) async fn recover_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let threadless = cx
            .update(|app| -> Result<_, String> {
                let mut owner = owner.borrow_mut();
                let _driver = owner.reserve_interrupted_exit_driver(request)?;
                if cancellation.is_cancelled() {
                    return Err("Interrupted Exit preparation was cancelled".into());
                }
                let shells = owner.process.windows.shells();
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
                                "Interrupted Exit requires the sole retained threadless window"
                                    .into(),
                            );
                        }
                        threadless = Some(window);
                    } else if controller.composer_mount().is_none() {
                        return Err("Interrupted Exit requires selected windows".into());
                    }
                }
                Ok(threadless)
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
}
