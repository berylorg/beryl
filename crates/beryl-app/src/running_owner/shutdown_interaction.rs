use super::progress::RunningShutdownProgress;
use super::{RunningProcessOwner, RunningShutdownStatus};
use crate::{app_services::AppServiceShutdownProgress, main_window::MainWindowShellRoot};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    pub(crate) fn release_shutdown_interaction_gate(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<(), String> {
        let windows = {
            let owner = owner.borrow();
            if owner.shutdown.is_some()
                || owner.process.services.is_none()
                || !matches!(
                    owner.progress,
                    Some(RunningShutdownProgress::Settled(Ok(
                        AppServiceShutdownProgress::Failed { reopened: true, .. }
                    )))
                )
            {
                return Err(
                    "shutdown interaction release requires retained coherent reopening evidence"
                        .into(),
                );
            }
            owner
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>()
        };
        MainWindowShellRoot::release_shutdown_interaction_gates(&windows, app)
    }

    pub(crate) fn install_shutdown_interaction_gate(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<(), String> {
        let windows = {
            let owner = owner.borrow();
            if !matches!(
                owner.shutdown_status(),
                Some((
                    _,
                    _,
                    RunningShutdownStatus::Admitted | RunningShutdownStatus::WorkReady
                ))
            ) {
                return Err("shutdown interaction gating requires admitted intent".into());
            }
            owner
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>()
        };
        if windows.is_empty() {
            return Err("shutdown interaction gating has no published windows".into());
        }
        let mut failure = None;
        for window in windows {
            let result = window
                .update(app, |root, _, cx| {
                    root.set_shutdown_interaction_gated(true, cx)
                })
                .map_err(|error| format!("shutdown window is unavailable: {error}"))
                .and_then(|result| result);
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
