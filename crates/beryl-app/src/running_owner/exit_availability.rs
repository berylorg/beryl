use super::RunningProcessOwner;
use gpui::App;
use std::{cell::RefCell, rc::Rc, time::Duration};

impl RunningProcessOwner {
    fn project_exit_availability(owner: &Rc<RefCell<Self>>, app: &mut App) {
        let (reason, windows) = {
            let owner = owner.borrow();
            (
                owner.process.commands.disabled_reason(),
                owner
                    .process
                    .windows
                    .shells()
                    .iter()
                    .map(|shell| shell.window())
                    .collect::<Vec<_>>(),
            )
        };
        for window in windows {
            let _ = window.update(app, |root, _, cx| {
                root.set_exit_disabled_reason(reason, cx);
            });
        }
    }

    pub(super) fn observe_exit_availability(owner: &Rc<RefCell<Self>>, app: &mut App) {
        Self::project_exit_availability(owner, app);
        let weak = Rc::downgrade(owner);
        let task = app.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if !cx
                    .update(|app| {
                        let Some(owner) = weak.upgrade() else {
                            return false;
                        };
                        Self::project_exit_availability(&owner, app);
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        owner.borrow_mut().exit_availability = Some(task);
    }
}
