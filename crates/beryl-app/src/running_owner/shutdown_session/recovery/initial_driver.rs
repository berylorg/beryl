use super::*;
use crate::main_window::MainWindowShellRoot;
use beryl_home_store::CommandCancellation;
use gpui::{AsyncApp, WindowHandle};
use resident_windows_driver::ResidentRecoveryConfigurator;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) async fn recover_interrupted_exit(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        configure: impl FnMut(
            WindowHandle<MainWindowShellRoot>,
        ) -> Result<ResidentRecoveryConfigurator, String>,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (threadless, retired) = cx
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
}
