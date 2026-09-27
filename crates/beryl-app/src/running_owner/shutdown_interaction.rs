use super::{RunningProcessOwner, RunningShutdownStatus};
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    pub(crate) fn install_shutdown_interaction_gate(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<(), String> {
        let windows = {
            let owner = owner.borrow();
            if !matches!(
                owner.shutdown_status(),
                Some((_, _, RunningShutdownStatus::Admitted))
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
