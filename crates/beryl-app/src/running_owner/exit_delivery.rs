use super::RunningProcessOwner;
use crate::startup_owner::RunningExitRequest;
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    pub(crate) fn wait_for_exit(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, &mut App) + 'static,
    ) -> Result<(), String> {
        {
            let mut owner = owner.borrow_mut();
            if owner.waiting_for_exit {
                return Err("a running Exit wait is already pending".to_owned());
            }
            owner.waiting_for_exit = true;
        }
        let retained = owner.clone();
        app.spawn(async move |cx| {
            let request =
                std::future::poll_fn(|cx| retained.borrow_mut().process.commands.poll_exit(cx))
                    .await;
            retained.borrow_mut().waiting_for_exit = false;
            let _ = cx.update(|app| completed(&retained, request, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn finish_exit(owner: &Rc<RefCell<Self>>, request: &RunningExitRequest) -> bool {
        let result = owner
            .borrow_mut()
            .process
            .commands
            .finish_exit_deferred_wake(request);
        let Ok(wake) = result else {
            return false;
        };
        if let Some(wake) = wake {
            wake.wake();
        }
        true
    }
}
